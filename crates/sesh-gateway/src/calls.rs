//! Executor-facing `/call` (contracts/executor.md): every connector call a
//! script makes arrives here with the execution's capability. The executor
//! is untrusted, so everything is checked again here.

use crate::Gateway;
use crate::approvals::{self, Ask, Gate};
use crate::audit::Entry;
use crate::capability::Refusal;
use crate::redact::summarize;
use crate::schema;
use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

fn refuse(status: u16, reason: &str) -> Reply {
    Reply {
        status,
        body: json!({"ok": false, "error": reason}),
    }
}

/// Handles one forwarded call. `capability` comes from the
/// `X-Sesh-Capability` header; `body` is `{connector, method, args}`.
pub async fn handle(gw: &Gateway, capability: Option<&str>, body: &Value) -> Reply {
    let connector = body["connector"].as_str().unwrap_or("").to_string();
    let method = body["method"].as_str().unwrap_or("").to_string();
    let args = body.get("args").cloned().unwrap_or(json!({}));
    let mut entry = Entry {
        agent: "unknown".into(),
        execution: None,
        connector: Some(connector.clone()).filter(|c| !c.is_empty()),
        method: Some(method.clone()).filter(|m| !m.is_empty()),
        args_summary: summarize(&args),
        outcome: "refused",
        reason: None,
    };

    let refused = |entry: &mut Entry, status: u16, reason: &str| {
        entry.reason = Some(reason.to_string());
        (status, reason.to_string())
    };

    let outcome: Result<Value, (u16, String)> = 'checks: {
        let Some(cap) = capability.filter(|c| !c.is_empty()) else {
            break 'checks Err(refused(&mut entry, 403, "missing capability"));
        };
        let grant = match gw.caps.use_for_call(cap, gw.settings.max_calls) {
            Ok(g) => g,
            Err(Refusal::Unknown) => {
                break 'checks Err(refused(&mut entry, 403, "invalid capability"));
            }
            Err(Refusal::Expired) => {
                break 'checks Err(refused(&mut entry, 403, "expired capability"));
            }
            Err(Refusal::TooManyCalls) => {
                break 'checks Err(refused(
                    &mut entry,
                    429,
                    "too many connector calls in one execution",
                ));
            }
        };
        entry.agent = grant.agent.clone();
        entry.execution = Some(grant.execution.clone());
        let Some(agent) = gw.agents.get(&grant.agent) else {
            break 'checks Err(refused(&mut entry, 403, "agent no longer exists"));
        };
        if !agent.connectors.contains(&connector) {
            break 'checks Err(refused(
                &mut entry,
                403,
                "connector not granted to this agent",
            ));
        }
        let Some(cdesc) = gw.connectors.iter().find(|c| c.name == connector) else {
            break 'checks Err(refused(&mut entry, 404, "no such method"));
        };
        let Some(desc) = cdesc.methods.get(&method) else {
            break 'checks Err(refused(&mut entry, 404, "no such method"));
        };
        if let Err(problem) = schema::validate(&desc.input, &args) {
            break 'checks Err(refused(&mut entry, 400, &problem));
        }
        // Credentials the connector declares, read only here (FR-011).
        let mut creds = std::collections::BTreeMap::new();
        for name in &cdesc.credentials {
            match gw
                .secrets
                .get(&sesh_core::credential_key(&connector, name))
                .await
            {
                Ok(Some(v)) => {
                    creds.insert(name.clone(), v);
                }
                Ok(None) => {
                    break 'checks Err(refused(
                        &mut entry,
                        409,
                        &format!("{connector} is not set up yet (credential {name} is missing)"),
                    ));
                }
                Err(_) => {
                    break 'checks Err(refused(&mut entry, 503, "credential store unavailable"));
                }
            }
        }
        // Approval-gated methods: the gateway decides, whatever the executor
        // claims (ADR 0007).
        let mut approved = None;
        if desc.requires_approval {
            let ask = Ask {
                agent: &grant.agent,
                connector: &connector,
                method: &method,
                description: &desc.description,
                args: &args,
            };
            match approvals::gate(
                &gw.approvals,
                &gw.notifier,
                &gw.approval_limits(),
                &ask,
                approvals::now(),
            )
            .await
            {
                Gate::Run(r) => approved = Some(r.id),
                Gate::Pending(r) => {
                    gw.note_pending(&grant.execution, &r);
                    entry.reason = Some(format!("approval pending {}", r.id));
                    break 'checks Err((
                        202,
                        format!(
                            "{connector}.{method} needs the person's approval; they have been asked (request {})",
                            r.id
                        ),
                    ));
                }
                Gate::Refused(why) => break 'checks Err(refused(&mut entry, 403, &why)),
            }
        }
        match gw.upstream.call(&connector, &method, &args, &creds).await {
            Ok(v) => {
                entry.outcome = "allowed";
                let mut notes = vec![];
                if let Some(id) = &approved {
                    notes.push(format!("approved {id}"));
                }
                for name in creds.keys() {
                    notes.push(format!("credential {connector}/{name}"));
                }
                entry.reason = (!notes.is_empty()).then(|| notes.join("; "));
                Ok(v)
            }
            Err(e) => {
                entry.outcome = "failed";
                entry.reason = Some(e.clone());
                Err((502, e))
            }
        }
    };

    if gw.audit.write(&entry).await.is_err() {
        // Never hand back a result that wasn't recorded (FR-010).
        return refuse(503, "audit log unavailable; call not completed");
    }
    match outcome {
        Ok(result) => Reply {
            status: 200,
            body: json!({"ok": true, "result": result}),
        },
        Err((status, reason)) => refuse(status, &reason),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::audit::Audit;
    use crate::capability::Capabilities;
    use crate::connectors::{self, Upstream};
    use crate::{Agents, Gateway};
    use sesh_core::{Agent, AgentsFile, GatewaySettings, Home, token_hash};
    use std::time::Duration;

    pub fn gateway(audit: Audit, settings: GatewaySettings) -> Gateway {
        let agent = |n: &str, c: &[&str]| Agent {
            owner: sesh_core::HOUSEHOLD.into(),
            name: n.into(),
            token_sha256: token_hash(&format!("tok-{n}")),
            connectors: c.iter().map(|s| s.to_string()).collect(),
        };
        Gateway {
            agents: Agents::fixed(AgentsFile {
                agents: vec![
                    agent("alice", &["search", "geo", "testgate", "testcred"]),
                    agent("bob", &["search"]),
                ],
            }),
            connectors: connectors::all(true),
            caps: Capabilities::default(),
            audit,
            upstream: Upstream::new(settings.clone()).unwrap(),
            settings,
            executor_url: "http://executor-{agent}:8000".into(),
            approvals: crate::approvals::Store::memory(),
            notifier: crate::notify::Notifier::memory(),
            secrets: crate::secrets::SecretStore::memory(),
            admin_token: Some("admin-token".into()),
            pending: Default::default(),
        }
    }

    fn settings_with(base: &str) -> GatewaySettings {
        let mut s = GatewaySettings {
            home: Some(Home {
                latitude: 59.9,
                longitude: 10.7,
            }),
            ..Default::default()
        };
        s.upstreams.open_meteo = base.into();
        s.upstreams.searxng = base.into();
        s.upstreams.test = base.into();
        s
    }

    async fn fake_upstream() -> String {
        use axum::{Json, Router, routing::get};
        let app = Router::new()
            .route(
                "/search",
                get(|| async {
                    Json(json!({"results": [{"title": "T", "url": "u", "content": "c"}]}))
                }),
            )
            .route(
                "/testcred",
                get(|h: axum::http::HeaderMap| async move {
                    let ok = h.get("authorization").and_then(|v| v.to_str().ok())
                        == Some("Bearer right-value");
                    if ok {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::UNAUTHORIZED
                    }
                }),
            )
            .route(
                "/v1/forecast",
                get(|| async {
                    Json(
                        json!({"daily": {"time": ["2026-11-27"], "temperature_2m_max": [5.0],
                    "temperature_2m_min": [1.0], "precipitation_sum": [0.0], "weather_code": [0]}}),
                    )
                }),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn call(c: &str, m: &str, args: Value) -> Value {
        json!({"connector": c, "method": m, "args": args})
    }

    #[tokio::test]
    async fn allowed_call_reaches_upstream_and_is_audited_once() {
        let gw = gateway(Audit::memory(), settings_with(&fake_upstream().await));
        let cap = gw.caps.issue("alice", "exec_1", Duration::from_secs(30));
        let r = handle(&gw, Some(&cap), &call("geo", "forecast", json!({}))).await;
        assert_eq!(r.status, 200, "{:?}", r.body);
        assert_eq!(r.body["result"]["days"][0]["summary"], "clear");
        let r = handle(
            &gw,
            Some(&cap),
            &call("search", "web", json!({"query": "x"})),
        )
        .await;
        assert_eq!(r.body["result"][0]["title"], "T");
        let e = gw.audit.entries();
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].agent.as_str(), e[0].outcome), ("alice", "allowed"));
        assert_eq!(e[0].execution.as_deref(), Some("exec_1"));
    }

    #[tokio::test]
    async fn every_refusal_is_refused_and_audited() {
        let gw = gateway(Audit::memory(), settings_with("http://127.0.0.1:9"));
        let alice = gw.caps.issue("alice", "exec_a", Duration::from_secs(30));
        let bob = gw.caps.issue("bob", "exec_b", Duration::from_secs(30));
        let expired = gw.caps.issue("alice", "exec_x", Duration::from_millis(1));
        tokio::time::sleep(Duration::from_millis(5)).await;
        let cases = [
            (
                None,
                call("search", "web", json!({"query": "x"})),
                403,
                "missing capability",
            ),
            (
                Some("forged"),
                call("search", "web", json!({"query": "x"})),
                403,
                "invalid capability",
            ),
            (
                Some(expired.as_str()),
                call("search", "web", json!({"query": "x"})),
                403,
                "expired",
            ),
            (
                Some(bob.as_str()),
                call("geo", "forecast", json!({})),
                403,
                "not granted",
            ),
            (
                Some(alice.as_str()),
                call("geo", "launch", json!({})),
                404,
                "no such method",
            ),
            (
                Some(alice.as_str()),
                call("testgate", "act", json!({"approved": true})),
                400,
                "not allowed",
            ),
            (
                Some(alice.as_str()),
                call("search", "web", json!({"query": "x", "user": "bob"})),
                400,
                "not allowed",
            ),
            (
                Some(alice.as_str()),
                call("search", "web", json!({})),
                400,
                "required",
            ),
        ];
        for (cap, body, status, why) in &cases {
            let r = handle(&gw, *cap, body).await;
            assert_eq!(r.status, *status, "{body}: {:?}", r.body);
            assert!(
                r.body["error"].as_str().unwrap().contains(why),
                "{:?}",
                r.body
            );
        }
        let e = gw.audit.entries();
        assert_eq!(e.len(), cases.len());
        assert!(
            e.iter()
                .all(|x| x.outcome == "refused" && x.reason.is_some())
        );
        // The capability string never reaches the audit log.
        assert!(!format!("{e:?}").contains(&alice));
    }

    #[tokio::test]
    async fn call_limit_and_broken_audit_refuse() {
        let mut s = settings_with(&fake_upstream().await);
        s.max_calls = 1;
        let gw = gateway(Audit::memory(), s.clone());
        let cap = gw.caps.issue("alice", "e", Duration::from_secs(30));
        assert_eq!(
            handle(&gw, Some(&cap), &call("geo", "forecast", json!({})))
                .await
                .status,
            200
        );
        assert_eq!(
            handle(&gw, Some(&cap), &call("geo", "forecast", json!({})))
                .await
                .status,
            429
        );

        let broken = Gateway {
            audit: Audit::Memory {
                entries: Default::default(),
                broken: true,
            },
            ..gateway(Audit::memory(), s)
        };
        let cap = broken.caps.issue("alice", "e", Duration::from_secs(30));
        let r = handle(&broken, Some(&cap), &call("geo", "forecast", json!({}))).await;
        assert_eq!(r.status, 503);
        assert!(r.body.get("result").is_none());
    }

    #[tokio::test]
    async fn gated_calls_wait_then_run_once() {
        let gw = gateway(Audit::memory(), settings_with(&fake_upstream().await));
        let cap = gw.caps.issue("alice", "exec_g", Duration::from_secs(30));
        let act = call("testgate", "act", json!({"note": "hi"}));
        let r = handle(&gw, Some(&cap), &act).await;
        assert_eq!(r.status, 202, "{:?}", r.body);
        assert_eq!(gw.notifier.sent().len(), 1);
        let pending = gw.take_pending("exec_g");
        assert_eq!(pending.len(), 1);
        let id = pending[0].id.clone();
        let secret = gw.notifier.sent()[0]
            .link
            .rsplit('#')
            .next()
            .unwrap()
            .to_string();
        crate::approvals::decide(&gw.approvals, &id, &secret, true, crate::approvals::now())
            .await
            .unwrap();
        let r = handle(&gw, Some(&cap), &act).await;
        assert_eq!(r.body["result"]["done"], true, "{:?}", r.body);
        let r = handle(&gw, Some(&cap), &act).await;
        assert_eq!(r.status, 202, "an approval runs once");
        let e = gw.audit.entries();
        assert_eq!(
            e[0].reason.as_deref(),
            Some(format!("approval pending {id}").as_str())
        );
        assert_eq!(e[1].outcome, "allowed");
        assert_eq!(
            e[1].reason.as_deref(),
            Some(format!("approved {id}").as_str())
        );
        assert!(!format!("{e:?}").contains(&secret));
    }

    #[tokio::test]
    async fn credentials_reach_only_their_upstream() {
        let gw = gateway(Audit::memory(), settings_with(&fake_upstream().await));
        let cap = gw.caps.issue("alice", "exec_c", Duration::from_secs(30));
        let check = call("testcred", "check", json!({}));
        let r = handle(&gw, Some(&cap), &check).await;
        assert_eq!(r.status, 409);
        assert!(r.body["error"].as_str().unwrap().contains("not set up yet"));
        gw.secrets
            .put("TESTCRED__TOKEN", "right-value")
            .await
            .unwrap();
        let r = handle(&gw, Some(&cap), &check).await;
        assert_eq!(r.body, json!({"ok": true, "result": {"ok": true}}));
        assert!(!r.body.to_string().contains("right-value"));
        let e = gw.audit.entries();
        assert_eq!(e[1].reason.as_deref(), Some("credential testcred/TOKEN"));
        assert!(!format!("{e:?}").contains("right-value"));
        let mut broken = gateway(Audit::memory(), settings_with("http://127.0.0.1:9"));
        broken.secrets = crate::secrets::SecretStore::broken();
        let cap = broken
            .caps
            .issue("alice", "exec_d", Duration::from_secs(30));
        let r = handle(&broken, Some(&cap), &check).await;
        assert_eq!(
            (r.status, r.body["error"].as_str().unwrap()),
            (503, "credential store unavailable")
        );
        // Calls that need no credential still work without the store.
        let r = handle(&broken, Some(&cap), &call("geo", "forecast", json!({}))).await;
        assert_ne!(r.status, 503);
    }
}
