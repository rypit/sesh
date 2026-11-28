//! The live view (specs/010 research.md R6, ADR 0014): a small view-only
//! window in the chat message and a larger window to take over, both
//! served on the approvals listener. Frames and input pass through the
//! gateway; a grant is bound to one Open WebUI user and one agent, its
//! secret travels in the URL fragment and is stored hashed.

use crate::Gateway;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Grant {
    pub id: String,
    secret_sha256: String,
    pub agent: String,
    pub webui_user: String,
    last_used: Instant,
}

/// Live-view grants (data-model.md "Live view grant"), in memory: a
/// gateway restart ends every live view, which only means the window says
/// so.
#[derive(Default)]
pub struct Grants {
    map: Mutex<HashMap<String, Grant>>,
}

impl Grants {
    /// A grant for `webui_user` to watch `agent`'s browser; returns
    /// (id, secret). One live grant per (agent, user) is reused.
    pub fn issue(&self, agent: &str, webui_user: &str, ttl: Duration) -> (String, String) {
        let mut m = self.map.lock().expect("lock");
        m.retain(|_, g| g.last_used.elapsed() < ttl);
        let id = crate::approvals::new_id();
        let secret = crate::approvals::new_secret();
        m.insert(
            id.clone(),
            Grant {
                id: id.clone(),
                secret_sha256: crate::approvals::sha256_hex(&secret),
                agent: agent.into(),
                webui_user: webui_user.into(),
                last_used: Instant::now(),
            },
        );
        (id, secret)
    }

    /// The grant, if `secret` matches and it hasn't expired; marks it used.
    pub fn check(&self, id: &str, secret: &str, ttl: Duration) -> Option<Grant> {
        let mut m = self.map.lock().expect("lock");
        let g = m.get_mut(id)?;
        if g.last_used.elapsed() >= ttl {
            m.remove(id);
            return None;
        }
        if !crate::approvals::ct_eq(&crate::approvals::sha256_hex(secret), &g.secret_sha256) {
            return None;
        }
        g.last_used = Instant::now();
        Some(g.clone())
    }
}

pub fn router() -> Router<Arc<Gateway>> {
    Router::new()
        .route("/live/{id}", get(view_page))
        .route("/live/{id}/control", get(control_page))
        .route("/live/{id}/ws", get(ws))
}

fn html(body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src blob:; connect-src 'self' ws: wss:",
            ),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

async fn view_page(Path(_id): Path<String>) -> Response {
    html(VIEW_HTML)
}

async fn control_page(Path(_id): Path<String>) -> Response {
    html(CONTROL_HTML)
}

async fn ws(
    State(gw): State<Arc<Gateway>>,
    Path(id): Path<String>,
    up: WebSocketUpgrade,
) -> Response {
    up.max_message_size(64 * 1024)
        .on_upgrade(move |socket| async move { serve(gw, id, socket).await })
}

async fn send_json(socket: &mut WebSocket, v: Value) -> bool {
    socket
        .send(Message::Text(v.to_string().into()))
        .await
        .is_ok()
}

/// Maps a key the person pressed to CDP's key event fields.
fn special_key(key: &str) -> Option<(&'static str, i64, Option<&'static str>)> {
    Some(match key {
        "Enter" => ("Enter", 13, Some("\r")),
        "Backspace" => ("Backspace", 8, None),
        "Tab" => ("Tab", 9, None),
        "Escape" => ("Escape", 27, None),
        "Delete" => ("Delete", 46, None),
        "ArrowLeft" => ("ArrowLeft", 37, None),
        "ArrowUp" => ("ArrowUp", 38, None),
        "ArrowRight" => ("ArrowRight", 39, None),
        "ArrowDown" => ("ArrowDown", 40, None),
        "Home" => ("Home", 36, None),
        "End" => ("End", 35, None),
        "PageUp" => ("PageUp", 33, None),
        "PageDown" => ("PageDown", 34, None),
        _ => return None,
    })
}

async fn serve(gw: Arc<Gateway>, id: String, mut socket: WebSocket) {
    let ttl = Duration::from_secs(gw.settings.browser.live_ttl_secs);
    // The first message carries the secret (from the URL fragment).
    let first = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
    let hello: Value = match first {
        Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str(&t).unwrap_or(Value::Null),
        _ => return,
    };
    let secret = hello["secret"].as_str().unwrap_or("");
    let control = hello["control"] == true;
    let Some(grant) = gw.browser.live.check(&id, secret, ttl) else {
        let _ = send_json(
            &mut socket,
            json!({"type": "error", "error": "this live view has expired"}),
        )
        .await;
        audit(&gw, "unknown", "live.refused", "expired or wrong secret").await;
        return;
    };
    let Some(s) = gw.browser.existing(&grant.agent).await else {
        let _ = send_json(&mut socket, json!({"type": "status", "state": "closed"})).await;
        return;
    };
    audit(
        &gw,
        &grant.agent,
        if control { "live.control" } else { "live.view" },
        &format!("Open WebUI user {}", grant.webui_user),
    )
    .await;
    if s.add_viewer().await.is_err() {
        return;
    }
    let mut frames = s.frames.subscribe();
    let first_frame = match s.last_frame() {
        Some(f) => Some(f.as_ref().clone()),
        None => s.screenshot().await.ok(),
    };
    if let Some(f) = first_frame {
        let _ = socket.send(Message::Binary(f.into())).await;
    }
    let status = |s: &super::session::Session| -> Value {
        match s.control() {
            super::session::Control::Agent => json!({"type": "status", "state": "agent"}),
            super::session::Control::Person { grant: g, .. } if g == grant.id => {
                json!({"type": "status", "state": "you"})
            }
            super::session::Control::Person { .. } => json!({"type": "status", "state": "person"}),
        }
    };
    let _ = send_json(&mut socket, status(&s)).await;
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            f = frames.recv() => match f {
                Ok(f) => {
                    if socket.send(Message::Binary(f.as_ref().clone().into())).await.is_err() { break; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            _ = tick.tick() => {
                if gw.browser.live.check(&id, secret, ttl).is_none() || s.is_closed() { break; }
                if !send_json(&mut socket, status(&s)).await { break; }
            }
            m = socket.recv() => {
                let Some(Ok(m)) = m else { break };
                let Message::Text(t) = m else { continue };
                if !control { continue; }
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                handle_input(&gw, &s, &grant, &v).await;
                let _ = send_json(&mut socket, status(&s)).await;
            }
        }
    }
    s.remove_viewer().await;
    if control && s.hand_back(&grant.id) {
        audit(&gw, &grant.agent, "live.hand_back", "the window closed").await;
    }
}

/// Input from the control window, only while this grant is in control
/// (FR-010). Typed text is never logged or shown to the agent (FR-012).
async fn handle_input(gw: &Gateway, s: &super::session::Session, grant: &Grant, v: &Value) {
    let mine =
        matches!(s.control(), super::session::Control::Person { grant: g, .. } if g == grant.id);
    match v["type"].as_str().unwrap_or("") {
        "take" => {
            s.take_over(&grant.id);
            audit(
                gw,
                &grant.agent,
                "live.take_over",
                &format!("Open WebUI user {}", grant.webui_user),
            )
            .await;
        }
        "give" => {
            if s.hand_back(&grant.id) {
                audit(gw, &grant.agent, "live.hand_back", "handed back").await;
            }
        }
        _ if !mine => {}
        "click" | "scroll" => {
            let Ok((w, h)) = s.viewport().await else {
                return;
            };
            let x = v["fx"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) * w;
            let y = v["fy"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) * h;
            if v["type"] == "click" {
                let _ = s.click_at(x, y).await;
            } else {
                let _ = s
                    .scroll(x, y, v["dy"].as_f64().unwrap_or(0.0).clamp(-3000.0, 3000.0))
                    .await;
            }
        }
        "text" => {
            let t: String = v["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(1000)
                .collect();
            let _ = s.insert_text(&t).await;
        }
        "key" => {
            if let Some((key, code, text)) = special_key(v["key"].as_str().unwrap_or("")) {
                let _ = s.key_event(key, code, text).await;
            }
        }
        _ => {}
    }
}

async fn audit(gw: &Gateway, agent: &str, what: &str, reason: &str) {
    let _ = gw
        .audit
        .write(&crate::audit::Entry {
            agent: agent.into(),
            execution: None,
            connector: Some("browser".into()),
            method: Some(what.into()),
            args_summary: json!({}),
            outcome: "allowed",
            reason: Some(reason.into()),
        })
        .await;
}

/// The address of the live window for a grant (the embed's URL).
pub fn url(gw: &Gateway, id: &str, secret: &str) -> String {
    format!(
        "{}/live/{id}#{secret}",
        gw.settings.approvals_base_url.trim_end_matches('/')
    )
}

pub fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

const VIEW_HTML: &str = include_str!("live_view.html");
const CONTROL_HTML: &str = include_str!("live_control.html");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_need_the_secret_and_expire() {
        let g = Grants::default();
        let ttl = Duration::from_secs(60);
        let (id, secret) = g.issue("house", "u1", ttl);
        assert_eq!(g.check(&id, &secret, ttl).unwrap().webui_user, "u1");
        assert!(g.check(&id, "wrong", ttl).is_none());
        assert!(g.check("made-up", &secret, ttl).is_none());
        assert!(g.check(&id, &secret, Duration::ZERO).is_none(), "expired");
        assert!(g.check(&id, &secret, ttl).is_none(), "and gone");
        let (id2, _) = g.issue("house", "u1", ttl);
        let m = g.map.lock().unwrap();
        assert!(
            !m[&id2].secret_sha256.is_empty() && m.values().all(|x| x.secret_sha256.len() == 64),
            "only hashes are stored"
        );
    }

    #[test]
    fn only_known_keys_are_sent_as_key_events() {
        assert_eq!(special_key("Enter"), Some(("Enter", 13, Some("\r"))));
        assert_eq!(special_key("a"), None, "printable keys arrive as text");
        assert_eq!(special_key("F12"), None);
    }

    #[test]
    fn pages_carry_no_secret_and_a_strict_policy() {
        for p in [VIEW_HTML, CONTROL_HTML] {
            assert!(
                p.contains("location.hash"),
                "the secret comes from the fragment"
            );
            assert!(
                !p.contains("http://") && !p.contains("https://"),
                "no outside resources"
            );
        }
    }
}
