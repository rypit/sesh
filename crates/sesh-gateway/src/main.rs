//! sesh-gateway binary. Configuration from the environment:
//!
//! - `SESH_GATEWAY_CONFIG`: path to `gateway.toml` ([`sesh_core::GatewaySettings`])
//! - `SESH_AGENTS_FILE`: path to `agents.toml`
//! - `SESH_AUDIT_DATABASE_URL`: Postgres URL for the audit role, `memory`, or
//!   `file:/var/log/sesh-eval/<name>` (the eval gateway only)
//! - `SESH_EXECUTOR_URL`: executor URL template, default `http://executor-{agent}:8000`
//! - `SESH_GATEWAY_TEST_CONNECTORS=1`: add the test-only `testgate` connector
//! - `SESH_ALLOWED_HOSTS`: extra Host names for the MCP endpoint (comma separated)
//! - `SESH_AGENT_LISTEN`: agent listener addresses (comma separated); default
//!   loopback and the default-route address, never the executor networks
//! - `SESH_EXECUTOR_LISTEN`: executor listener, default `0.0.0.0:7001`
//! - `SESH_APPROVALS_LISTEN`: approval pages and `/admin` (comma separated);
//!   default loopback and the default-route address on 7002
//! - `SESH_APPROVALS_DATABASE_URL`: approvals store; default the audit URL
//!   when that is Postgres, else in memory
//! - `SESH_NTFY_URL`, `SESH_NTFY_USER`, `SESH_NTFY_PASSWORD`: the approval
//!   channel (the topic is `ntfy_topic` in the gateway config)
//! - `SESH_INFISICAL_URL`, `SESH_INFISICAL_CLIENT_ID`,
//!   `SESH_INFISICAL_CLIENT_SECRET`, `SESH_INFISICAL_PROJECT_ID`: the secret store
//! - `SESH_GATEWAY_ADMIN_TOKEN`: the operator's token for `/admin/*`

#![forbid(unsafe_code)]

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::Value;
use sesh_core::GatewaySettings;
use sesh_gateway::{
    Agents, Gateway, approvals, audit::Audit, auth, calls, connectors, listen, mcp::Tools, notify,
    pages, secrets,
};
use std::sync::Arc;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

async fn call_handler(
    State(gw): State<Arc<Gateway>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if headers.contains_key("authorization") {
        // Agent tokens are never accepted on the executor side.
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"ok": false, "error": "agent tokens aren't accepted here"})),
        );
    }
    let cap = headers
        .get("x-sesh-capability")
        .and_then(|v| v.to_str().ok());
    let r = calls::handle(&gw, cap, &body).await;
    (
        StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(r.body),
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let settings: GatewaySettings = match env("SESH_GATEWAY_CONFIG") {
        Some(p) => toml::from_str(&std::fs::read_to_string(&p)?)?,
        None => GatewaySettings::default(),
    };
    settings.validate()?;
    let agents = Agents::from_file(
        env("SESH_AGENTS_FILE")
            .unwrap_or_else(|| "/etc/sesh/agents.toml".into())
            .into(),
    );
    let audit_url = env("SESH_AUDIT_DATABASE_URL");
    let approvals = match env("SESH_APPROVALS_DATABASE_URL")
        .or_else(|| audit_url.clone().filter(|u| u.starts_with("postgres")))
    {
        Some(url) if url.starts_with("postgres") => approvals::Store::postgres(url),
        _ => approvals::Store::memory(),
    };
    let notifier = match env("SESH_NTFY_URL") {
        Some(url) => notify::Notifier::ntfy(
            &url,
            &settings.ntfy_topic,
            &env("SESH_NTFY_USER").unwrap_or_default(),
            &env("SESH_NTFY_PASSWORD").unwrap_or_default(),
        )?,
        // No channel: every gated call is refused (FR-006).
        None => notify::Notifier::broken(),
    };
    let secrets = match (
        env("SESH_INFISICAL_URL"),
        env("SESH_INFISICAL_CLIENT_ID"),
        env("SESH_INFISICAL_CLIENT_SECRET"),
        env("SESH_INFISICAL_PROJECT_ID"),
    ) {
        (Some(u), Some(id), Some(sec), Some(p)) => {
            secrets::SecretStore::infisical(&u, &id, &sec, &p)?
        }
        _ => secrets::SecretStore::None,
    };
    let audit = match audit_url.as_deref() {
        Some("memory") | None => Audit::memory(),
        Some(url) if url.starts_with("file:") => Audit::file(&url["file:".len()..])?,
        Some(url) => Audit::postgres(url.to_string()),
    };
    let gw = Arc::new(Gateway {
        agents,
        connectors: connectors::all(env("SESH_GATEWAY_TEST_CONNECTORS").as_deref() == Some("1")),
        caps: Default::default(),
        audit,
        approvals,
        notifier,
        secrets,
        admin_token: env("SESH_GATEWAY_ADMIN_TOKEN"),
        pending: Default::default(),
        upstream: connectors::Upstream::new(settings.clone())?,
        settings,
        executor_url: env("SESH_EXECUTOR_URL")
            .unwrap_or_else(|| "http://executor-{agent}:8000".into()),
    });

    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".into(),
        "::1".into(),
        "gateway".into(),
    ];
    hosts.extend(
        env("SESH_ALLOWED_HOSTS")
            .map(|h| h.split(',').map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    let tools = Tools {
        gw: gw.clone(),
        http: reqwest::Client::new(),
    };
    let mcp = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_allowed_hosts(hosts),
    );
    let agent_app = Router::new()
        .nest_service("/mcp", mcp)
        .layer(axum::middleware::from_fn_with_state(
            gw.clone(),
            auth::require_agent,
        ))
        .route("/health", get(|| async { "ok" }));
    let executor_app = Router::new()
        .route("/call", post(call_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(gw.clone());

    // Not on the executor networks: see the `listen` module.
    let agent_addrs = listen::agent_addrs(
        env("SESH_AGENT_LISTEN").as_deref(),
        listen::default_route_ip(),
    )
    .map_err(anyhow::Error::msg)?;
    let mut agent_listeners = Vec::new();
    for addr in &agent_addrs {
        agent_listeners.push(tokio::net::TcpListener::bind(addr).await?);
    }
    let approvals_app = pages::router(gw.clone());
    let approval_addrs = listen::addrs_on(
        7002,
        env("SESH_APPROVALS_LISTEN").as_deref(),
        listen::default_route_ip(),
    )
    .map_err(anyhow::Error::msg)?;
    let mut approval_listeners = Vec::new();
    for addr in &approval_addrs {
        approval_listeners.push(tokio::net::TcpListener::bind(addr).await?);
    }
    let exec_listener = tokio::net::TcpListener::bind(
        env("SESH_EXECUTOR_LISTEN").unwrap_or_else(|| "0.0.0.0:7001".into()),
    )
    .await?;
    tracing::info!(
        "sesh-gateway listening: agents {:?}, approvals {:?}, executors {}",
        agent_addrs,
        approval_addrs,
        exec_listener.local_addr()?
    );
    let mut servers = tokio::task::JoinSet::new();
    for l in agent_listeners {
        let app = agent_app.clone();
        servers.spawn(async move { axum::serve(l, app).await });
    }
    for l in approval_listeners {
        let app = approvals_app.clone();
        servers.spawn(async move { axum::serve(l, app).await });
    }
    servers.spawn(async move { axum::serve(exec_listener, executor_app).await });
    // Each server runs until it fails; one failing stops the gateway.
    if let Some(r) = servers.join_next().await {
        r??;
    }
    Ok(())
}
