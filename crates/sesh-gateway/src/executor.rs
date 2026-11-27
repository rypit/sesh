//! Client for an agent's executor (contracts/executor.md). The executor is
//! untrusted: the gateway enforces its own deadline and only trusts what it
//! re-checks on `/call`.

use serde_json::{Value, json};
use sesh_core::ConnectorDescriptor;
use std::time::Duration;

/// What the agent sees as the execution's status.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Completed { result: Value, logs: Value },
    Error { error: String, logs: Value },
    Timeout,
    ApprovalUnavailable,
}

/// Grace the gateway allows beyond the deadline before giving up itself.
pub const GRACE: Duration = Duration::from_secs(5);

pub async fn run(
    http: &reqwest::Client,
    url: &str,
    execution: &str,
    capability: &str,
    code: &str,
    connectors: &[ConnectorDescriptor],
    deadline: Duration,
) -> Outcome {
    let body = json!({
        "execution": execution,
        "capability": capability,
        "deadline_ms": deadline.as_millis() as u64,
        "code": code,
        "connectors": connectors,
    });
    let resp = http
        .post(format!("{}/execute", url.trim_end_matches('/')))
        .timeout(deadline + GRACE)
        .json(&body)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return Outcome::Timeout,
        Err(_) => {
            return Outcome::Error {
                error: "the executor is unavailable; try again".into(),
                logs: json!([]),
            };
        }
    };
    if resp.status().as_u16() == 504 {
        return Outcome::Timeout;
    }
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    interpret(&v)
}

pub fn interpret(v: &Value) -> Outcome {
    let logs = v.get("logs").cloned().unwrap_or(json!([]));
    match v["status"].as_str() {
        Some("completed") => Outcome::Completed {
            result: v.get("result").cloned().unwrap_or(Value::Null),
            logs,
        },
        Some("paused") => Outcome::ApprovalUnavailable,
        Some("error") => Outcome::Error {
            error: v["error"].as_str().unwrap_or("script failed").to_string(),
            logs,
        },
        _ => Outcome::Error {
            error: "the executor sent an unreadable reply".into(),
            logs: json!([]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_each_status() {
        assert!(matches!(
            interpret(&json!({"status": "completed", "result": 1})),
            Outcome::Completed { .. }
        ));
        assert_eq!(
            interpret(&json!({"status": "paused"})),
            Outcome::ApprovalUnavailable
        );
        assert!(matches!(
            interpret(&json!({"status": "error", "error": "x"})),
            Outcome::Error { .. }
        ));
        assert!(matches!(
            interpret(&json!("nonsense")),
            Outcome::Error { .. }
        ));
    }
}
