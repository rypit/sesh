//! A small Chrome DevTools Protocol client (specs/010 research.md R5):
//! commands by id over one WebSocket, events broadcast to subscribers,
//! flat sessions (`sessionId`) for pages.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// One CDP event: its method, params and session.
#[derive(Debug, Clone)]
pub struct Event {
    pub method: String,
    pub params: Value,
    pub session: Option<String>,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Clone)]
pub struct Cdp {
    next: Arc<AtomicU64>,
    out: mpsc::Sender<String>,
    pending: Pending,
    events: broadcast::Sender<Event>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Cdp {
    /// Connects to a browser's WebSocket debugger URL.
    pub async fn connect(ws_url: &str) -> Result<Cdp, String> {
        let (ws, _) = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async(ws_url),
        )
        .await
        .map_err(|_| "the browser didn't answer".to_string())?
        .map_err(|e| format!("can't reach the browser: {e}"))?;
        let (mut sink, mut stream) = ws.split();
        let (out, mut rx) = mpsc::channel::<String>(256);
        let pending: Pending = Arc::default();
        let (events, _) = broadcast::channel(1024);
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if sink.send(Message::Text(m.into())).await.is_err() {
                    break;
                }
            }
        });
        let (p, ev, c) = (pending.clone(), events.clone(), closed.clone());
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                let Message::Text(t) = msg else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&t) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = p.lock().await.remove(&id) {
                        let r = match v.get("error") {
                            Some(e) => {
                                Err(e["message"].as_str().unwrap_or("CDP error").to_string())
                            }
                            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(r);
                    }
                } else if let Some(m) = v.get("method").and_then(Value::as_str) {
                    let _ = ev.send(Event {
                        method: m.to_string(),
                        params: v.get("params").cloned().unwrap_or(Value::Null),
                        session: v.get("sessionId").and_then(Value::as_str).map(String::from),
                    });
                }
            }
            c.store(true, Ordering::SeqCst);
            for (_, tx) in p.lock().await.drain() {
                let _ = tx.send(Err("the browser connection closed".into()));
            }
        });
        Ok(Cdp {
            next: Arc::new(AtomicU64::new(1)),
            out,
            pending,
            events,
            closed,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Sends a command (to a page session when `session` is set) and waits
    /// up to `timeout` for its result.
    pub async fn call_in(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if self.is_closed() {
            return Err("the browser connection closed".into());
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        if self.out.send(msg.to_string()).await.is_err() {
            self.pending.lock().await.remove(&id);
            return Err("the browser connection closed".into());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("the browser connection closed".into()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(format!("the browser didn't answer {method} in time"))
            }
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.call_in(None, method, params, Duration::from_secs(30))
            .await
    }
}

/// The browser's WebSocket debugger URL from `/json/version`. Chromium
/// accepts only an IP address or `localhost` as the Host header, so the
/// host is resolved first and the URL rewritten to the address.
pub async fn ws_url(host: &str, port: u16) -> Result<String, String> {
    let addr = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| format!("can't find the browser ({host})"))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("no address for the browser ({host})"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let v: Value = client
        .get(format!("http://{addr}/json/version"))
        .send()
        .await
        .map_err(|_| "the browser isn't running".to_string())?
        .json()
        .await
        .map_err(|_| "the browser's answer wasn't understood".to_string())?;
    let ws = v["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("the browser gave no debugger address")?;
    let path = ws
        .split_once("/devtools/")
        .map(|(_, p)| p)
        .ok_or("unexpected debugger address")?;
    Ok(format!("ws://{addr}/devtools/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A fake browser: answers `Echo` with its params, `Fail` with an
    /// error, and sends one event first.
    async fn fake() -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
            while let Some(Ok(Message::Text(t))) = ws.next().await {
                let v: Value = serde_json::from_str(&t).unwrap();
                let id = v["id"].clone();
                let reply = match v["method"].as_str().unwrap() {
                    "Echo" => {
                        ws.send(Message::Text(
                            json!({"method": "Page.loaded", "params": {"n": 1}, "sessionId": v["sessionId"]})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                        json!({"id": id, "result": {"got": v["params"], "session": v["sessionId"]}})
                    }
                    "Slow" => continue,
                    _ => json!({"id": id, "error": {"code": -32000, "message": "no such thing"}}),
                };
                ws.send(Message::Text(reply.to_string().into()))
                    .await
                    .unwrap();
            }
        });
        format!("ws://{addr}/devtools/browser/x")
    }

    #[tokio::test]
    async fn commands_get_their_results_and_events_are_broadcast() {
        let c = Cdp::connect(&fake().await).await.unwrap();
        let mut ev = c.subscribe();
        let r = c
            .call_in(Some("s1"), "Echo", json!({"a": 1}), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(r, json!({"got": {"a": 1}, "session": "s1"}));
        let e = ev.recv().await.unwrap();
        assert_eq!(
            (e.method.as_str(), e.session.as_deref()),
            ("Page.loaded", Some("s1"))
        );
        assert_eq!(
            c.call("Fail", json!({})).await.unwrap_err(),
            "no such thing"
        );
        let slow = c
            .call_in(None, "Slow", json!({}), Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(slow.contains("in time"), "{slow}");
    }

    #[tokio::test]
    async fn an_unreachable_browser_is_an_error() {
        assert!(
            Cdp::connect("ws://127.0.0.1:9/devtools/browser/x")
                .await
                .is_err()
        );
        assert!(ws_url("127.0.0.1", 9).await.is_err());
    }
}
