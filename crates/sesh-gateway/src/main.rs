//! sesh-gateway binary. Configuration from the environment:
//!
//! - `SESH_GATEWAY_CONFIG`: path to `gateway.toml` ([`sesh_core::GatewaySettings`])
//! - `SESH_AGENTS_FILE`: path to `agents.toml`
//! - `SESH_AUDIT_DATABASE_URL`: Postgres URL for the audit role, or `memory`
//! - `SESH_EXECUTOR_URL`: executor URL template, default `http://executor-{agent}:8000`
//! - `SESH_GATEWAY_TEST_CONNECTORS=1`: add the test-only `testgate` connector
//! - `SESH_ALLOWED_HOSTS`: extra Host names for the MCP endpoint (comma separated)
//! - `SESH_AGENT_LISTEN`: agent listener addresses (comma separated); default
//!   loopback and the default-route address, never the executor networks
//! - `SESH_EXECUTOR_LISTEN`: executor listener, default `0.0.0.0:7001`

#![forbid(unsafe_code)]

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::Value;
use sesh_core::GatewaySettings;
use sesh_gateway::{Agents, Gateway, audit::Audit, auth, calls, connectors, listen, mcp::Tools};
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
    let audit = match env("SESH_AUDIT_DATABASE_URL").as_deref() {
        Some("memory") | None => Audit::memory(),
        Some(url) => Audit::postgres(url.to_string()),
    };
    let gw = Arc::new(Gateway {
        agents,
        connectors: connectors::all(env("SESH_GATEWAY_TEST_CONNECTORS").as_deref() == Some("1")),
        caps: Default::default(),
        audit,
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
    let exec_listener = tokio::net::TcpListener::bind(
        env("SESH_EXECUTOR_LISTEN").unwrap_or_else(|| "0.0.0.0:7001".into()),
    )
    .await?;
    tracing::info!(
        "sesh-gateway listening: agents {:?}, executors {}",
        agent_addrs,
        exec_listener.local_addr()?
    );
    let mut servers = tokio::task::JoinSet::new();
    for l in agent_listeners {
        let app = agent_app.clone();
        servers.spawn(async move { axum::serve(l, app).await });
    }
    servers.spawn(async move { axum::serve(exec_listener, executor_app).await });
    // Each server runs until it fails; one failing stops the gateway.
    if let Some(r) = servers.join_next().await {
        r??;
    }
    Ok(())
}
