//! One agent's browser session (specs/010 data-model.md): the CDP
//! connection, its page, who is in control, the live view's frames, and
//! the downloads it has captured.

use super::cdp::{Cdp, Event};
use super::egress;
use crate::refs::{Attachment, Source};
use base64::Engine;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify, broadcast};

pub const PAGE_JS: &str = include_str!("page.js");
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Downloads the gateway keeps (spec Assumptions).
pub const DOWNLOAD_MAX_BYTES: u64 = 25 * 1024 * 1024;
pub const DOWNLOADS_PER_AGENT: usize = 20;
const DOWNLOAD_TYPES: &[(&str, &str)] = &[
    ("application/pdf", "pdf"),
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("text/plain", "txt"),
    ("text/csv", "csv"),
    ("application/msword", "doc"),
    ("application/vnd.ms-excel", "xls"),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "docx",
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xlsx",
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "pptx",
    ),
];

enum BodyError {
    TooLarge,
    Failed,
}

/// Who drives the browser.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Agent,
    Person { grant: String, since: Instant },
}

/// A download the agent was given a reference for.
#[derive(Debug, Clone, PartialEq)]
pub struct Download {
    pub r#ref: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
}

pub struct Session {
    pub agent: String,
    pub owner: String,
    pub cdp: Cdp,
    page: String,
    world: Mutex<Option<(String, i64)>>,
    control: std::sync::Mutex<Control>,
    handed_back: Notify,
    /// JPEG frames for the live view.
    pub frames: broadcast::Sender<Arc<Vec<u8>>>,
    last_frame: std::sync::Mutex<Option<Arc<Vec<u8>>>>,
    viewers: AtomicUsize,
    /// Downloads captured since the last time the agent was told.
    fresh: std::sync::Mutex<Vec<Download>>,
    /// Refusals (too large, not allowed) since the last call.
    refused: std::sync::Mutex<Vec<String>>,
    pub refs: Arc<crate::refs::Refs>,
    ref_ttl: Duration,
}

/// The type a download is kept as, if allowed: the declared type, or the
/// file name's extension when the server says only "octet-stream".
pub fn download_type(content_type: &str, filename: &str) -> Option<&'static str> {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if let Some((t, _)) = DOWNLOAD_TYPES.iter().find(|(t, _)| *t == ct) {
        return Some(t);
    }
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if ct.is_empty() || ct == "application/octet-stream" || ct == "binary/octet-stream" {
        let ext = if ext == "jpeg" {
            "jpg".to_string()
        } else {
            ext
        };
        return DOWNLOAD_TYPES
            .iter()
            .find(|(_, e)| *e == ext)
            .map(|(t, _)| *t);
    }
    None
}

/// The file name from a `Content-Disposition` header, or the URL's last
/// path segment; never a path.
pub fn download_name(disposition: &str, url: &str) -> String {
    let from_header = disposition.split(';').map(str::trim).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        let k = k.trim().to_ascii_lowercase();
        if k == "filename*" {
            let v = v.rsplit("''").next()?;
            Some(
                url::form_urlencoded::parse(format!("x={v}").as_bytes())
                    .next()?
                    .1
                    .to_string(),
            )
        } else if k == "filename" {
            Some(v.trim_matches('"').to_string())
        } else {
            None
        }
    });
    let name = from_header.unwrap_or_else(|| {
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.path_segments()?.next_back().map(str::to_string))
            .unwrap_or_default()
    });
    let name: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let name = name.trim().trim_start_matches('.').to_string();
    if name.is_empty() {
        "download".into()
    } else {
        name
    }
}

fn header<'a>(headers: &'a Value, name: &str) -> &'a str {
    headers
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
}

impl Session {
    /// Connects to the agent's browser and prepares its page.
    pub async fn open(
        agent: &str,
        owner: &str,
        ws_url: &str,
        refs: Arc<crate::refs::Refs>,
        ref_ttl: Duration,
    ) -> Result<Arc<Session>, String> {
        let cdp = Cdp::connect(ws_url).await?;
        let targets = cdp.call("Target.getTargets", json!({})).await?;
        let page_target = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["type"] == "page" && t["browserContextId"].is_string())
            .and_then(|t| t["targetId"].as_str().map(String::from));
        let target = match page_target {
            Some(t) => t,
            None => cdp
                .call("Target.createTarget", json!({"url": "about:blank"}))
                .await?["targetId"]
                .as_str()
                .ok_or("no page")?
                .to_string(),
        };
        let page = cdp
            .call(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
            )
            .await?["sessionId"]
            .as_str()
            .ok_or("can't attach to the page")?
            .to_string();
        // Downloads never land in the container: the gateway captures them.
        let _ = cdp
            .call("Browser.setDownloadBehavior", json!({"behavior": "deny"}))
            .await;
        let (frames, _) = broadcast::channel(4);
        let s = Arc::new(Session {
            agent: agent.into(),
            owner: owner.into(),
            cdp,
            page,
            world: Mutex::new(None),
            control: std::sync::Mutex::new(Control::Agent),
            handed_back: Notify::new(),
            frames,
            last_frame: std::sync::Mutex::new(None),
            viewers: AtomicUsize::new(0),
            fresh: std::sync::Mutex::new(vec![]),
            refused: std::sync::Mutex::new(vec![]),
            refs,
            ref_ttl,
        });
        for m in ["Page.enable", "Runtime.enable"] {
            s.page_call(m, json!({})).await?;
        }
        s.page_call(
            "Fetch.enable",
            json!({"patterns": [{"urlPattern": "*", "resourceType": "Document", "requestStage": "Response"}]}),
        )
        .await?;
        let me = Arc::downgrade(&s);
        let mut events = s.cdp.subscribe();
        tokio::spawn(async move {
            while let Ok(ev) = events.recv().await {
                let Some(s) = me.upgrade() else { break };
                if ev.session.as_deref() != Some(s.page.as_str()) {
                    continue;
                }
                s.on_event(ev).await;
            }
        });
        Ok(s)
    }

    pub fn is_closed(&self) -> bool {
        self.cdp.is_closed()
    }

    async fn page_call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.cdp
            .call_in(Some(&self.page), method, params, Duration::from_secs(30))
            .await
    }

    async fn on_event(self: &Arc<Self>, ev: Event) {
        match ev.method.as_str() {
            "Fetch.requestPaused" => {
                let s = self.clone();
                tokio::spawn(async move { s.on_response(ev.params).await });
            }
            "Page.screencastFrame" => {
                let id = ev.params["sessionId"].clone();
                let _ = self
                    .cdp
                    .call_in(
                        Some(&self.page),
                        "Page.screencastFrameAck",
                        json!({"sessionId": id}),
                        Duration::from_secs(5),
                    )
                    .await;
                if let Some(Ok(jpeg)) = ev.params["data"].as_str().map(|d| B64.decode(d)) {
                    let f = Arc::new(jpeg);
                    *self.last_frame.lock().expect("lock") = Some(f.clone());
                    let _ = self.frames.send(f);
                }
            }
            "Page.frameNavigated" if ev.params["frame"]["parentId"].is_null() => {
                *self.world.lock().await = None;
            }
            _ => {}
        }
    }

    /// A document response: kept as a download, or passed on to the page.
    async fn on_response(&self, p: Value) {
        let id = p["requestId"].clone();
        let headers = &p["responseHeaders"];
        let disposition = header(headers, "content-disposition");
        let content_type = header(headers, "content-type");
        let url = p["request"]["url"].as_str().unwrap_or("");
        let is_download = disposition.to_ascii_lowercase().starts_with("attachment")
            || (!content_type.starts_with("text/html")
                && !content_type.starts_with("application/xhtml")
                && !content_type.is_empty()
                && !content_type.starts_with("text/plain")
                && !content_type.starts_with("image/svg"));
        if !is_download {
            let _ = self
                .page_call("Fetch.continueRequest", json!({"requestId": id}))
                .await;
            return;
        }
        let name = download_name(disposition, url);
        let refuse = |why: String| {
            let id = id.clone();
            async move {
                self.refused.lock().expect("lock").push(why);
                let _ = self
                    .page_call(
                        "Fetch.failRequest",
                        json!({"requestId": id, "errorReason": "Aborted"}),
                    )
                    .await;
            }
        };
        let Some(mime) = download_type(content_type, &name) else {
            return refuse(format!("{name}: that kind of file isn't allowed")).await;
        };
        let declared: u64 = header(headers, "content-length").parse().unwrap_or(0);
        if declared > DOWNLOAD_MAX_BYTES {
            return refuse(format!("{name}: that download is too large (limit 25 MB)")).await;
        }
        // Read the body as a stream, 1 MB at a time, stopping past the
        // limit: a server may not say the size up front (research.md R9).
        let bytes = match self.read_body(&id).await {
            Ok(b) => Some(b),
            Err(BodyError::TooLarge) => {
                return refuse(format!("{name}: that download is too large (limit 25 MB)")).await;
            }
            Err(BodyError::Failed) => None,
        };
        let Some(bytes) = bytes else {
            return refuse(format!("{name}: the download didn't arrive")).await;
        };
        if bytes.len() as u64 > DOWNLOAD_MAX_BYTES {
            return refuse(format!("{name}: that download is too large (limit 25 MB)")).await;
        }
        let size = bytes.len() as u64;
        // Keep at most DOWNLOADS_PER_AGENT: forget the oldest.
        let mut held = self.refs.downloads_of(&self.agent);
        while held.len() >= DOWNLOADS_PER_AGENT {
            let (r, _) = held.remove(0);
            self.refs.drop_ref(&r);
        }
        let r = self.refs.issue(
            Attachment {
                agent: self.agent.clone(),
                owner: self.owner.clone(),
                source: Source::Download {
                    bytes: Arc::new(bytes),
                },
                filename: name.clone(),
                mime: mime.to_string(),
                size,
            },
            self.ref_ttl,
        );
        self.fresh.lock().expect("lock").push(Download {
            r#ref: r,
            filename: name,
            mime: mime.into(),
            size,
        });
        let _ = self
            .page_call(
                "Fetch.failRequest",
                json!({"requestId": id, "errorReason": "Aborted"}),
            )
            .await;
    }

    /// A paused response's body through `Fetch.takeResponseBodyAsStream`,
    /// in pieces small enough for one CDP message each.
    async fn read_body(&self, id: &Value) -> Result<Vec<u8>, BodyError> {
        let handle = self
            .page_call("Fetch.takeResponseBodyAsStream", json!({"requestId": id}))
            .await
            .map_err(|_| BodyError::Failed)?["stream"]
            .clone();
        let mut out = Vec::new();
        let result = loop {
            let chunk = match self
                .page_call("IO.read", json!({"handle": handle, "size": 1024 * 1024}))
                .await
            {
                Ok(c) => c,
                Err(_) => break Err(BodyError::Failed),
            };
            let data = chunk["data"].as_str().unwrap_or("");
            if chunk["base64Encoded"] == true {
                match B64.decode(data) {
                    Ok(b) => out.extend_from_slice(&b),
                    Err(_) => break Err(BodyError::Failed),
                }
            } else {
                out.extend_from_slice(data.as_bytes());
            }
            if out.len() as u64 > DOWNLOAD_MAX_BYTES {
                break Err(BodyError::TooLarge);
            }
            if chunk["eof"] == true {
                break Ok(out);
            }
        };
        let _ = self.page_call("IO.close", json!({"handle": handle})).await;
        result
    }

    /// Downloads captured (and refusals) since the last call.
    pub fn take_downloads(&self) -> (Vec<Download>, Vec<String>) {
        (
            std::mem::take(&mut *self.fresh.lock().expect("lock")),
            std::mem::take(&mut *self.refused.lock().expect("lock")),
        )
    }

    /// The gateway's isolated world in the current document.
    async fn world(&self) -> Result<i64, String> {
        let tree = self.page_call("Page.getFrameTree", json!({})).await?;
        let frame = &tree["frameTree"]["frame"];
        let loader = frame["loaderId"].as_str().unwrap_or("").to_string();
        let mut w = self.world.lock().await;
        if let Some((l, id)) = w.as_ref()
            && *l == loader
        {
            return Ok(*id);
        }
        let ctx = self
            .page_call(
                "Page.createIsolatedWorld",
                json!({"frameId": frame["id"], "worldName": "sesh", "grantUniveralAccess": false}),
            )
            .await?["executionContextId"]
            .as_i64()
            .ok_or("no script context")?;
        *w = Some((loader, ctx));
        Ok(ctx)
    }

    /// Runs one of `page.js`'s operations.
    pub async fn run(&self, op: &str, args: Value) -> Result<Value, String> {
        let mut last = String::new();
        for _ in 0..3 {
            let ctx = self.world().await?;
            let expr = format!("({PAGE_JS})({}, {})", json!(op), args);
            let r = self
                .page_call(
                    "Runtime.evaluate",
                    json!({"expression": expr, "contextId": ctx, "returnByValue": true, "awaitPromise": false}),
                )
                .await;
            match r {
                Ok(v) if v.get("exceptionDetails").is_none() => {
                    let out = v["result"]["value"].clone();
                    if let Some(e) = out.get("error").and_then(Value::as_str) {
                        return Err(e.to_string());
                    }
                    return Ok(out);
                }
                // The document changed under us: a new world next time.
                Ok(v) => {
                    last = v["exceptionDetails"]["text"]
                        .as_str()
                        .unwrap_or("script error")
                        .into()
                }
                Err(e) => last = e,
            }
            *self.world.lock().await = None;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Err(format!("the page couldn't be read ({last})"))
    }

    /// Waits until the document has loaded, up to `limit`.
    pub async fn settle(&self, limit: Duration) {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_millis(250)).await;
        while start.elapsed() < limit {
            if let Ok(v) = self.run("ready", json!({})).await
                && v["state"] == "complete"
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Opens `url` in the agent's page.
    pub async fn navigate(&self, url: &str, timeout: Duration) -> Result<(), String> {
        let r = self.page_call("Page.navigate", json!({"url": url})).await?;
        if let Some(e) = r["errorText"].as_str()
            && !e.is_empty()
            && e != "net::ERR_ABORTED"
        {
            return Err(format!("the page didn't load ({e})"));
        }
        self.settle(timeout).await;
        Ok(())
    }

    pub async fn click_at(&self, x: f64, y: f64) -> Result<(), String> {
        for t in ["mouseMoved", "mousePressed", "mouseReleased"] {
            self.page_call(
                "Input.dispatchMouseEvent",
                json!({"type": t, "x": x, "y": y, "button": "left", "clickCount": 1}),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn insert_text(&self, text: &str) -> Result<(), String> {
        self.page_call("Input.insertText", json!({"text": text}))
            .await
            .map(|_| ())
    }

    /// Presses and releases a special key (Enter, Backspace, arrows…).
    pub async fn key_event(&self, key: &str, vk: i64, text: Option<&str>) -> Result<(), String> {
        let mut down = json!({"type": if text.is_some() { "keyDown" } else { "rawKeyDown" }, "key": key, "code": key, "windowsVirtualKeyCode": vk});
        if let Some(t) = text {
            down["text"] = json!(t);
        }
        self.page_call("Input.dispatchKeyEvent", down).await?;
        self.page_call(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": key, "code": key, "windowsVirtualKeyCode": vk}),
        )
        .await
        .map(|_| ())
    }

    /// The page's visible size in CSS pixels, for the live view's input.
    pub async fn viewport(&self) -> Result<(f64, f64), String> {
        let m = self.page_call("Page.getLayoutMetrics", json!({})).await?;
        let v = &m["cssVisualViewport"];
        Ok((
            v["clientWidth"].as_f64().unwrap_or(1280.0),
            v["clientHeight"].as_f64().unwrap_or(800.0),
        ))
    }

    pub async fn scroll(&self, x: f64, y: f64, dy: f64) -> Result<(), String> {
        self.page_call(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": 0, "deltaY": dy}),
        )
        .await
        .map(|_| ())
    }

    // ---- control ----

    pub fn control(&self) -> Control {
        self.control.lock().expect("lock").clone()
    }

    pub fn take_over(&self, grant: &str) {
        *self.control.lock().expect("lock") = Control::Person {
            grant: grant.into(),
            since: Instant::now(),
        };
    }

    /// Hands control back to the agent (a no-op if `grant` isn't in control).
    pub fn hand_back(&self, grant: &str) -> bool {
        let mut c = self.control.lock().expect("lock");
        if matches!(&*c, Control::Person { grant: g, .. } if g == grant) {
            *c = Control::Agent;
            self.handed_back.notify_waiters();
            return true;
        }
        false
    }

    /// For the agent's calls: waits while a person is in control (FR-010),
    /// handing back automatically after `max_takeover`.
    pub async fn wait_for_agent(
        &self,
        wait: Duration,
        max_takeover: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + wait;
        loop {
            let notified = self.handed_back.notified();
            match self.control() {
                Control::Agent => return Ok(()),
                Control::Person { grant, since } if since.elapsed() >= max_takeover => {
                    self.hand_back(&grant);
                    return Ok(());
                }
                Control::Person { .. } => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("the person is in control of the browser; try again later".into());
            }
            let _ = tokio::time::timeout(left.min(Duration::from_secs(1)), notified).await;
        }
    }

    // ---- live view ----

    /// A viewer joined: start the screencast if it's the first.
    pub async fn add_viewer(&self) -> Result<(), String> {
        if self.viewers.fetch_add(1, Ordering::SeqCst) == 0 {
            self.page_call(
                "Page.startScreencast",
                json!({"format": "jpeg", "quality": 60, "maxWidth": 1280, "maxHeight": 800, "everyNthFrame": 1}),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn remove_viewer(&self) {
        if self.viewers.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = self.page_call("Page.stopScreencast", json!({})).await;
        }
    }

    pub fn last_frame(&self) -> Option<Arc<Vec<u8>>> {
        self.last_frame.lock().expect("lock").clone()
    }

    /// A one-off screenshot (the first frame before the screencast's).
    pub async fn screenshot(&self) -> Result<Vec<u8>, String> {
        let v = self
            .page_call(
                "Page.captureScreenshot",
                json!({"format": "jpeg", "quality": 60}),
            )
            .await?;
        B64.decode(v["data"].as_str().unwrap_or(""))
            .map_err(|_| "bad screenshot".to_string())
    }
}

/// Reads `url` in a fresh browser context that shares nothing with the
/// agent's profile (FR-002), and disposes of it.
pub async fn read_fresh(
    cdp: &Cdp,
    proxy: &str,
    url: &str,
    max_bytes: usize,
    timeout: Duration,
) -> Result<Value, String> {
    let ctx = cdp
        .call(
            "Target.createBrowserContext",
            json!({"disposeOnDetach": true, "proxyServer": proxy}),
        )
        .await?["browserContextId"]
        .as_str()
        .ok_or("no browser context")?
        .to_string();
    let out = async {
        let _ = cdp
            .call(
                "Browser.setDownloadBehavior",
                json!({"behavior": "deny", "browserContextId": ctx}),
            )
            .await;
        let target = cdp
            .call(
                "Target.createTarget",
                json!({"url": "about:blank", "browserContextId": ctx}),
            )
            .await?["targetId"]
            .as_str()
            .ok_or("no page")?
            .to_string();
        let sess = cdp
            .call(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
            )
            .await?["sessionId"]
            .as_str()
            .ok_or("can't attach")?
            .to_string();
        let call = |m: &'static str, p: Value| {
            let sess = sess.clone();
            async move {
                cdp.call_in(Some(&sess), m, p, Duration::from_secs(30))
                    .await
            }
        };
        let nav = call("Page.navigate", json!({"url": url})).await?;
        if let Some(e) = nav["errorText"].as_str()
            && !e.is_empty()
        {
            return Err(format!("the page didn't load ({e})"));
        }
        let start = Instant::now();
        let expr = |op: &str, args: Value| format!("({PAGE_JS})({}, {})", json!(op), args);
        loop {
            let r = call(
                "Runtime.evaluate",
                json!({"expression": expr("ready", json!({})), "returnByValue": true}),
            )
            .await;
            if r.as_ref()
                .is_ok_and(|v| v["result"]["value"]["state"] == "complete")
                || start.elapsed() > timeout
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let frame = call("Page.getFrameTree", json!({})).await?;
        let world = call(
            "Page.createIsolatedWorld",
            json!({"frameId": frame["frameTree"]["frame"]["id"], "worldName": "sesh"}),
        )
        .await?["executionContextId"]
            .clone();
        let v = call(
            "Runtime.evaluate",
            json!({"expression": expr("extract", json!({"max_bytes": max_bytes})), "contextId": world, "returnByValue": true}),
        )
        .await?;
        let page = v["result"]["value"].clone();
        if page["url"]
            .as_str()
            .is_some_and(|u| u.starts_with("chrome-error:"))
        {
            return Err("the page didn't load".into());
        }
        Ok(page)
    };
    let r = out.await;
    let _ = cdp
        .call(
            "Target.disposeBrowserContext",
            json!({"browserContextId": ctx}),
        )
        .await;
    r
}

/// The egress refusal text, re-exported for the connector.
pub fn refused() -> String {
    egress::REFUSED.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_types_are_limited_and_inferred_only_from_octet_streams() {
        assert_eq!(
            download_type("application/pdf", "x"),
            Some("application/pdf")
        );
        assert_eq!(
            download_type("application/octet-stream", "bill.PDF"),
            Some("application/pdf")
        );
        assert_eq!(download_type("", "photo.jpeg"), Some("image/jpeg"));
        assert_eq!(download_type("application/x-msdownload", "setup.exe"), None);
        assert_eq!(download_type("application/octet-stream", "setup.exe"), None);
        assert_eq!(
            download_type("text/html", "bill.pdf"),
            None,
            "the declared type wins"
        );
        assert_eq!(download_type("application/zip", "a.zip"), None);
    }

    #[test]
    fn download_names_never_carry_a_path() {
        assert_eq!(
            download_name("attachment; filename=\"bill.pdf\"", ""),
            "bill.pdf"
        );
        assert_eq!(
            download_name("attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf", ""),
            "résumé.pdf"
        );
        assert_eq!(
            download_name("attachment; filename=\"../../etc/passwd\"", ""),
            "passwd"
        );
        assert_eq!(
            download_name("attachment", "https://x.test/a/b/c.pdf?x=1"),
            "c.pdf"
        );
        assert_eq!(
            download_name("attachment; filename=\".hidden\"", ""),
            "hidden"
        );
        assert_eq!(download_name("", "https://x.test/"), "download");
    }
}
