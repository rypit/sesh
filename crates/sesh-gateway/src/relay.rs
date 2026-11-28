//! The chat listener (specs/010 research.md R2, ADR 0014): Open WebUI
//! talks to each agent runtime through the gateway, which forwards the
//! request unchanged and streams the reply back, adding one `embeds`
//! event (the live browser window) when the agent uses its browser during
//! the request. The runtime checks its own API key; the relay adds the
//! window only to a successful reply, and only for an Open WebUI user it
//! can verify from the signed `X-OpenWebUI-User-Jwt` header.

use crate::Gateway;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use futures_util::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;

pub const USER_HEADER: &str = "x-openwebui-user-jwt";

/// The Open WebUI user id from its signed header (HS256, `sub`, `exp`).
pub fn verify_user(token: &str, secret: &str, now: u64) -> Option<String> {
    if secret.is_empty() {
        return None;
    }
    let mut parts = token.split('.');
    let (h, p, sig) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header: Value = serde_json::from_slice(&b64.decode(h).ok()?).ok()?;
    if header["alg"] != "HS256" {
        return None;
    }
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(format!("{h}.{p}").as_bytes());
    mac.verify_slice(&b64.decode(sig).ok()?).ok()?;
    let claims: Value = serde_json::from_slice(&b64.decode(p).ok()?).ok()?;
    if claims["exp"].as_u64().is_none_or(|e| e < now) {
        return None;
    }
    claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Splits an SSE byte stream into whole events, so an injected event
/// never lands inside another.
#[derive(Default)]
pub struct Events {
    buf: Vec<u8>,
}

impl Events {
    /// Adds bytes; returns the complete events (each ending in a blank
    /// line) now available.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(chunk);
        let mut out = vec![];
        loop {
            let pos = find_boundary(&self.buf);
            let Some((end, len)) = pos else { break };
            out.push(self.buf.drain(..end + len).collect());
        }
        out
    }

    /// Whatever is left when the stream ends.
    pub fn rest(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

fn find_boundary(b: &[u8]) -> Option<(usize, usize)> {
    let lf = b.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2));
    let crlf = b.windows(4).position(|w| w == b"\r\n\r\n").map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(c)) => Some(if a.0 <= c.0 { a } else { c }),
        (a, c) => a.or(c),
    }
}

/// The SSE event that adds the live window to the Open WebUI message.
pub fn embed_event(url: &str) -> Vec<u8> {
    format!(
        "data: {}\n\n",
        json!({"event": {"type": "embeds", "data": {"embeds": [url]}}})
    )
    .into_bytes()
}

pub fn router() -> Router<Arc<Gateway>> {
    Router::new()
        .route("/agent/{agent}/v1/chat/completions", post(chat))
        .route("/agent/{agent}/v1/models", get(models))
        .route("/health", get(|| async { "ok" }))
}

fn upstream(gw: &Gateway, agent: &str) -> Option<String> {
    // Only agents that exist and have a runtime.
    let a = gw.agents.get(agent)?;
    a.runtime.as_ref()?;
    Some(gw.settings.browser.chat_upstream.replace("{agent}", agent))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .no_proxy()
        .build()
        .expect("http client")
}

fn forwarded(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (k, v) in headers {
        let n = k.as_str();
        if n == "authorization"
            || n == "content-type"
            || n == "accept"
            || n.starts_with("x-hermes-")
        {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

async fn models(
    State(gw): State<Arc<Gateway>>,
    Path(agent): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(up) = upstream(&gw, &agent) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match client()
        .get(format!("{up}/v1/models"))
        .headers(forwarded(&headers))
        .send()
        .await
    {
        Ok(r) => {
            let status =
                StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            (
                status,
                [("content-type", "application/json")],
                r.bytes().await.unwrap_or_default(),
            )
                .into_response()
        }
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

async fn chat(
    State(gw): State<Arc<Gateway>>,
    Path(agent): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(up) = upstream(&gw, &agent) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let now = crate::approvals::now();
    let secret = std::env::var("SESH_WEBUI_JWT_SECRET").unwrap_or_default();
    let user = headers
        .get(USER_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|t| verify_user(t, &secret, now));
    // Listen before forwarding, so a browser call early in the turn counts.
    let mut in_use = gw.browser.in_use.subscribe();
    let r = match client()
        .post(format!("{up}/v1/chat/completions"))
        .headers(forwarded(&headers))
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_GATEWAY, "the agent isn't reachable").into_response(),
    };
    let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let ctype = r
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let streaming = ctype.starts_with("text/event-stream");
    if !status.is_success() || !streaming || user.is_none() {
        // Nothing to add: pass the reply through as it is.
        let stream = r.bytes_stream().map(|c| c.map_err(std::io::Error::other));
        return Response::builder()
            .status(status)
            .header("content-type", ctype)
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }
    let user = user.unwrap_or_default();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let gw2 = gw.clone();
    tokio::spawn(async move {
        let mut upstream = r.bytes_stream();
        let mut events = Events::default();
        let mut pending_embed: Option<Vec<u8>> = None;
        let mut embedded = false;
        loop {
            tokio::select! {
                used = in_use.recv(), if !embedded => {
                    if matches!(used, Ok(ref a) if *a == agent) {
                        let ttl = Duration::from_secs(gw2.settings.browser.live_ttl_secs);
                        let (id, secret) = gw2.browser.live.issue(&agent, &user, ttl);
                        pending_embed = Some(embed_event(&crate::browser::live::url(&gw2, &id, &secret)));
                        embedded = true;
                    }
                }
                chunk = upstream.next() => {
                    let Some(chunk) = chunk else { break };
                    let Ok(chunk) = chunk else { break };
                    for ev in events.push(&chunk) {
                        if tx.send(Ok(Bytes::from(ev))).await.is_err() { return; }
                        if let Some(e) = pending_embed.take()
                            && tx.send(Ok(Bytes::from(e))).await.is_err() { return; }
                    }
                }
            }
        }
        if let Some(e) = pending_embed.take() {
            let _ = tx.send(Ok(Bytes::from(e))).await;
        }
        let rest = events.rest();
        if !rest.is_empty() {
            let _ = tx.send(Ok(Bytes::from(rest))).await;
        }
    });
    let stream = tokio_stream_from(rx);
    Response::builder()
        .status(status)
        .header("content-type", ctype)
        .header("cache-control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn tokio_stream_from(
    mut rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> {
    futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(secret: &str, claims: Value) -> String {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let h = b64.encode(json!({"alg": "HS256", "typ": "JWT"}).to_string());
        let p = b64.encode(claims.to_string());
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{h}.{p}").as_bytes());
        format!("{h}.{p}.{}", b64.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn the_user_header_must_be_signed_and_current() {
        let t = jwt("s3cret", json!({"sub": "u1", "exp": 2000}));
        assert_eq!(verify_user(&t, "s3cret", 1000).as_deref(), Some("u1"));
        assert_eq!(verify_user(&t, "other", 1000), None, "wrong secret");
        assert_eq!(verify_user(&t, "s3cret", 3000), None, "expired");
        assert_eq!(verify_user(&t, "", 1000), None, "no secret configured");
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let parts: Vec<&str> = t.split('.').collect();
        let forged = format!(
            "{}.{}.{}",
            parts[0],
            b64.encode(json!({"sub": "admin", "exp": 2000}).to_string()),
            parts[2]
        );
        assert_eq!(
            verify_user(&forged, "s3cret", 1000),
            None,
            "tampered claims"
        );
        let none = format!(
            "{}.{}.",
            b64.encode(json!({"alg": "none"}).to_string()),
            parts[1]
        );
        assert_eq!(verify_user(&none, "s3cret", 1000), None, "alg none");
        assert_eq!(verify_user("garbage", "s3cret", 1000), None);
    }

    #[test]
    fn events_are_split_whole_whatever_the_chunking() {
        let stream = b"data: {\"a\":1}\n\ndata: {\"b\":2}\r\n\r\ndata: [DONE]\n\n";
        for size in [1, 3, 7, stream.len()] {
            let mut e = Events::default();
            let mut out = vec![];
            for c in stream.chunks(size) {
                out.extend(e.push(c));
            }
            assert_eq!(out.len(), 3, "chunk size {size}");
            assert_eq!(out.concat(), stream.to_vec());
            assert!(e.rest().is_empty());
        }
    }

    #[test]
    fn the_embed_event_is_what_open_webui_reads() {
        let e = String::from_utf8(embed_event("https://h/live/x#s")).unwrap();
        assert!(e.starts_with("data: ") && e.ends_with("\n\n"));
        let v: Value = serde_json::from_str(e.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(v["event"]["type"], "embeds");
        assert_eq!(v["event"]["data"]["embeds"][0], "https://h/live/x#s");
        assert!(v.get("choices").is_none(), "no content for the model");
    }
}
