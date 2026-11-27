//! Agent-facing auth: `Authorization: Bearer <token>`, compared by hash.
//! Unknown or missing tokens get 401 before any MCP processing, and the
//! refusal is audited.

use crate::Gateway;
use crate::audit::Entry;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde_json::json;
use std::sync::Arc;

/// The authenticated agent, placed in request extensions for the MCP handler.
#[derive(Clone, Debug)]
pub struct AgentName(pub String);

pub async fn require_agent(
    State(gw): State<Arc<Gateway>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    match token.and_then(|t| gw.agents.by_token(t)) {
        Some(agent) => {
            req.extensions_mut().insert(AgentName(agent.name));
            Ok(next.run(req).await)
        }
        None => {
            let _ = gw
                .audit
                .write(&Entry {
                    agent: "unknown".into(),
                    execution: None,
                    connector: None,
                    method: None,
                    args_summary: json!({}),
                    outcome: "refused",
                    reason: Some(
                        if token.is_some() {
                            "unknown token"
                        } else {
                            "missing token"
                        }
                        .into(),
                    ),
                })
                .await;
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
