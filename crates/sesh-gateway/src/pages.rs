//! The approvals listener (`:7002`; contracts/approvals.md and
//! credentials.md): the person's approval page and one-time credential
//! form, and the operator's `/admin/*`. Loopback and Tailscale only, never
//! on an agent or executor network; agent tokens are refused here.
//! Secrets come in the URL fragment and are POSTed, so they are never in
//! a request line or a log.

use crate::Gateway;
use crate::approvals::{self, Decided, Form};
use crate::audit::Entry;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn router(gw: Arc<Gateway>) -> Router {
    Router::new()
        .route("/approve/{id}", get(approve_page).post(approve))
        .route("/form/{id}", get(form_page).post(form_submit))
        .route("/admin/forms", post(admin_form))
        .route("/admin/credentials", get(admin_credentials))
        .route("/health", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(
            gw.clone(),
            refuse_agent_tokens,
        ))
        .with_state(gw)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn bearer(h: &HeaderMap) -> Option<&str> {
    h.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
}

async fn audit(
    gw: &Gateway,
    agent: &str,
    connector: Option<&str>,
    method: Option<&str>,
    outcome: &'static str,
    reason: String,
) {
    let _ = gw
        .audit
        .write(&Entry {
            agent: agent.into(),
            execution: None,
            connector: connector.map(str::to_string),
            method: method.map(str::to_string),
            args_summary: json!({}),
            outcome,
            reason: Some(reason),
        })
        .await;
}

/// FR-007: an agent's token is never accepted on this listener.
async fn refuse_agent_tokens(State(gw): State<Arc<Gateway>>, req: Request, next: Next) -> Response {
    if let Some(t) = bearer(req.headers())
        && let Some(agent) = gw.agents.by_token(t)
    {
        audit(
            &gw,
            &agent.name,
            None,
            None,
            "refused",
            "agent token on the approvals listener".into(),
        )
        .await;
        return (StatusCode::FORBIDDEN, "agent tokens aren't accepted here").into_response();
    }
    next.run(req).await
}

const PAGE_JS: &str = r#"<script>
const secret = location.hash.slice(1);
async function send(body) {
  const r = await fetch(location.pathname, {method: "POST", headers: {"content-type": "application/json"},
    body: JSON.stringify(Object.assign({secret}, body))});
  const t = await r.json().catch(() => ({}));
  document.getElementById("result").textContent = t.message || ("Error " + r.status);
  for (const b of document.querySelectorAll("button, input")) b.disabled = true;
}
</script>"#;

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
<title>{t}</title><style>body{{font:16px system-ui;margin:24px;max-width:640px}}pre{{white-space:pre-wrap;background:#f0efec;padding:12px;border-radius:8px}}\
button{{font-size:18px;padding:10px 20px;margin-right:12px}}@media(prefers-color-scheme:dark){{body{{background:#111;color:#eee}}pre{{background:#222}}}}</style>\
<h1>{t}</h1>{body}<p id=result></p>{PAGE_JS}",
        t = escape(title)
    ))
}

async fn approve_page(State(gw): State<Arc<Gateway>>, Path(id): Path<String>) -> Response {
    match gw.approvals.get(&id).await {
        Ok(Some(r)) => page(
            "Approve this action?",
            &format!(
                "<pre>{}</pre><p>State: {}</p><button onclick='send({{decision:\"approve\"}})'>Approve</button>\
<button onclick='send({{decision:\"deny\"}})'>Deny</button>",
                escape(&r.shown_text),
                escape(&r.state)
            ),
        )
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such request").into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "approvals unavailable").into_response(),
    }
}

fn msg(status: StatusCode, m: &str) -> Response {
    (status, Json(json!({"message": m}))).into_response()
}

async fn approve(
    State(gw): State<Arc<Gateway>>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> Response {
    let secret = b["secret"].as_str().unwrap_or("");
    let approve = match b["decision"].as_str() {
        Some("approve") => true,
        Some("deny") => false,
        _ => return msg(StatusCode::BAD_REQUEST, "decision must be approve or deny"),
    };
    let req = gw.approvals.get(&id).await.ok().flatten();
    let (agent, c, m) = req
        .as_ref()
        .map(|r| {
            (
                r.agent.clone(),
                Some(r.connector.clone()),
                Some(r.method.clone()),
            )
        })
        .unwrap_or(("unknown".into(), None, None));
    let (c, m) = (c.as_deref(), m.as_deref());
    match approvals::decide(&gw.approvals, &id, secret, approve, approvals::now()).await {
        Ok(Decided::Done(state)) => {
            let outcome = if state == "approved" {
                "allowed"
            } else {
                "refused"
            };
            audit(&gw, &agent, c, m, outcome, format!("approval {id} {state}")).await;
            msg(
                StatusCode::OK,
                if approve {
                    "Approved. The agent can now run it once."
                } else {
                    "Denied. It won't run."
                },
            )
        }
        Ok(Decided::Forbidden) => {
            audit(
                &gw,
                &agent,
                c,
                m,
                "refused",
                "approval refused: wrong secret".into(),
            )
            .await;
            msg(StatusCode::FORBIDDEN, "This link isn't valid.")
        }
        Ok(Decided::NotFound) => {
            audit(
                &gw,
                &agent,
                c,
                m,
                "refused",
                "approval refused: unknown".into(),
            )
            .await;
            msg(StatusCode::FORBIDDEN, "This link isn't valid.")
        }
        Ok(Decided::Conflict(state)) => {
            audit(
                &gw,
                &agent,
                c,
                m,
                "refused",
                format!("approval refused: already {state}"),
            )
            .await;
            msg(StatusCode::CONFLICT, &format!("Already {state}."))
        }
        Ok(Decided::Gone) => {
            audit(
                &gw,
                &agent,
                c,
                m,
                "refused",
                format!("approval {id} expired"),
            )
            .await;
            msg(StatusCode::GONE, "This request expired.")
        }
        Err(_) => msg(
            StatusCode::SERVICE_UNAVAILABLE,
            "Approvals are unavailable.",
        ),
    }
}

async fn form_page(State(gw): State<Arc<Gateway>>, Path(id): Path<String>) -> Response {
    match gw.approvals.get_form(&id).await {
        Ok(Some(f)) if f.used_at.is_some() => (StatusCode::GONE, "This link was already used.").into_response(),
        Ok(Some(f)) => page(
            &format!("Credential for {}", f.connector),
            &format!(
                "<p>Paste the value of <b>{}</b> for <b>{}</b>. It goes straight to sesh's secret store; no agent will see it.</p>\
<input id=v type=password autocomplete=off style='width:100%;font-size:16px;padding:8px'><p>\
<button onclick='send({{value: document.getElementById(\"v\").value}})'>Save</button></p>",
                escape(&f.name),
                escape(&f.connector)
            ),
        )
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such form").into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable").into_response(),
    }
}

async fn form_submit(
    State(gw): State<Arc<Gateway>>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> Response {
    let secret = b["secret"].as_str().unwrap_or("");
    let value = b["value"].as_str().unwrap_or("");
    let Ok(Some(f)) = gw.approvals.get_form(&id).await else {
        return msg(StatusCode::FORBIDDEN, "This link isn't valid.");
    };
    let what = format!("{}/{}", f.connector, f.name);
    if !approvals::ct_eq(&approvals::sha256_hex(secret), &f.secret_sha256) {
        audit(
            &gw,
            "operator",
            Some(&f.connector),
            None,
            "refused",
            format!("credential.fill refused {what}: wrong secret"),
        )
        .await;
        return msg(StatusCode::FORBIDDEN, "This link isn't valid.");
    }
    if f.used_at.is_some() {
        audit(
            &gw,
            "operator",
            Some(&f.connector),
            None,
            "refused",
            format!("credential.fill refused {what}: already used"),
        )
        .await;
        return msg(StatusCode::CONFLICT, "This link was already used.");
    }
    if f.expires_at <= approvals::now() {
        return msg(StatusCode::GONE, "This link expired.");
    }
    if value.is_empty() || value.len() > 8192 {
        return msg(
            StatusCode::BAD_REQUEST,
            "The value must be 1 to 8192 characters.",
        );
    }
    match gw.approvals.use_form(&id, approvals::now()).await {
        Ok(true) => {}
        _ => return msg(StatusCode::CONFLICT, "This link was already used."),
    }
    if gw
        .secrets
        .put(&sesh_core::credential_key(&f.connector, &f.name), value)
        .await
        .is_err()
    {
        return msg(
            StatusCode::SERVICE_UNAVAILABLE,
            "The secret store is unavailable; ask for a new link.",
        );
    }
    audit(
        &gw,
        "operator",
        Some(&f.connector),
        None,
        "allowed",
        format!("credential.fill {what}"),
    )
    .await;
    msg(StatusCode::OK, "Saved. You can close this page.")
}

fn admin_ok(gw: &Gateway, h: &HeaderMap) -> bool {
    match (&gw.admin_token, bearer(h)) {
        (Some(want), Some(got)) => approvals::ct_eq(want, got),
        _ => false,
    }
}

async fn admin_form(
    State(gw): State<Arc<Gateway>>,
    headers: HeaderMap,
    Json(b): Json<Value>,
) -> Response {
    if !admin_ok(&gw, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let connector = b["connector"].as_str().unwrap_or("");
    let name = b["name"].as_str().unwrap_or("");
    let declared = gw
        .connectors
        .iter()
        .any(|c| c.name == connector && c.credentials.iter().any(|n| n == name));
    if !declared {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "that connector declares no such credential"})),
        )
            .into_response();
    }
    let secret = approvals::new_secret();
    let at = approvals::now();
    let f = Form {
        id: approvals::new_id(),
        connector: connector.into(),
        name: name.into(),
        secret_sha256: approvals::sha256_hex(&secret),
        created_at: at,
        expires_at: at + gw.settings.form_ttl_secs,
        used_at: None,
    };
    if gw.approvals.insert_form(&f).await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    audit(
        &gw,
        "operator",
        Some(connector),
        None,
        "allowed",
        format!("credential.request {connector}/{name}"),
    )
    .await;
    let base = gw.settings.approvals_base_url.trim_end_matches('/');
    Json(json!({"url": format!("{base}/form/{}#{secret}", f.id), "expires_in_secs": gw.settings.form_ttl_secs})).into_response()
}

async fn admin_credentials(State(gw): State<Arc<Gateway>>, headers: HeaderMap) -> Response {
    if !admin_ok(&gw, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut out = vec![];
    for c in &gw.connectors {
        for n in &c.credentials {
            let state = match gw.secrets.get(&sesh_core::credential_key(&c.name, n)).await {
                Ok(Some(_)) => "set",
                Ok(None) => "missing",
                Err(_) => "unavailable",
            };
            out.push(json!({"connector": c.name, "name": n, "state": state}));
        }
    }
    Json(Value::Array(out)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Audit;
    use crate::calls::tests::gateway;
    use sesh_core::GatewaySettings;

    async fn serve(gw: Arc<Gateway>) -> String {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let app = router(gw);
        tokio::spawn(async move { axum::serve(l, app).await });
        url
    }

    #[tokio::test]
    async fn agent_tokens_are_refused_and_admin_needs_its_token() {
        let gw = Arc::new(gateway(Audit::memory(), GatewaySettings::default()));
        let url = serve(gw.clone()).await;
        let http = reqwest::Client::new();
        let r = http
            .get(format!("{url}/admin/credentials"))
            .bearer_auth("tok-alice")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 403);
        assert!(
            gw.audit
                .entries()
                .iter()
                .any(|e| e.reason.as_deref() == Some("agent token on the approvals listener"))
        );
        let r = http
            .get(format!("{url}/admin/credentials"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
        let r = http
            .get(format!("{url}/admin/credentials"))
            .bearer_auth("admin-token")
            .send()
            .await
            .unwrap();
        let list: Value = r.json().await.unwrap();
        assert_eq!(list[0]["name"], "TOKEN");
        assert_eq!(list[0]["state"], "missing");
    }

    #[tokio::test]
    async fn a_form_stores_once_and_never_echoes() {
        let gw = Arc::new(gateway(Audit::memory(), GatewaySettings::default()));
        let url = serve(gw.clone()).await;
        let http = reqwest::Client::new();
        let r = http
            .post(format!("{url}/admin/forms"))
            .bearer_auth("admin-token")
            .json(&json!({"connector": "search", "name": "TOKEN"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "only declared credentials");
        let r: Value = http
            .post(format!("{url}/admin/forms"))
            .bearer_auth("admin-token")
            .json(&json!({"connector": "testcred", "name": "TOKEN"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let link = r["url"].as_str().unwrap();
        let (path, secret) = link
            .trim_start_matches("http://127.0.0.1:7002")
            .split_once('#')
            .unwrap();
        let post = |s: &str, v: &str| {
            http.post(format!("{url}{path}"))
                .json(&json!({"secret": s, "value": v}))
                .send()
        };
        assert_eq!(post("wrong", "x").await.unwrap().status(), 403);
        let ok = post(secret, "s3cret-value-1").await.unwrap();
        assert_eq!(ok.status(), 200);
        assert!(!ok.text().await.unwrap().contains("s3cret-value-1"));
        assert_eq!(
            post(secret, "other").await.unwrap().status(),
            409,
            "one use"
        );
        assert_eq!(
            gw.secrets.get("TESTCRED__TOKEN").await.unwrap().as_deref(),
            Some("s3cret-value-1")
        );
        assert!(!format!("{:?}", gw.audit.entries()).contains("s3cret-value-1"));
        assert!(!format!("{:?}", gw.audit.entries()).contains(secret));
    }

    #[tokio::test]
    async fn the_page_escapes_the_text_and_holds_no_secret() {
        let gw = Arc::new(gateway(Audit::memory(), GatewaySettings::default()));
        let ask = approvals::Ask {
            agent: "alice",
            connector: "testgate",
            method: "act",
            description: "A side effect.",
            args: &json!({"note": "<script>alert(1)</script>"}),
        };
        let approvals::Gate::Pending(r) = approvals::gate(
            &gw.approvals,
            &gw.notifier,
            &gw.approval_limits(),
            &ask,
            approvals::now(),
        )
        .await
        else {
            panic!()
        };
        let url = serve(gw.clone()).await;
        let html = reqwest::get(format!("{url}/approve/{}", r.id))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            html.contains("&lt;script&gt;alert(1)&lt;/script&gt;")
                && !html.contains("<script>alert(1)")
        );
        let secret = gw.notifier.sent()[0]
            .link
            .rsplit('#')
            .next()
            .unwrap()
            .to_string();
        assert!(!html.contains(&secret));
        let http = reqwest::Client::new();
        let r2 = http
            .post(format!("{url}/approve/{}", r.id))
            .json(&json!({"secret": secret, "decision": "approve"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r2.status(), 200);
        let again = http
            .post(format!("{url}/approve/{}", r.id))
            .json(&json!({"secret": secret, "decision": "approve"}))
            .send()
            .await
            .unwrap();
        assert_eq!(again.status(), 409);
        let e = gw.audit.entries();
        assert!(
            e.iter()
                .any(|x| x.reason.as_deref() == Some(&format!("approval {} approved", r.id)))
        );
        assert!(!format!("{e:?}").contains(&secret));
    }
}
