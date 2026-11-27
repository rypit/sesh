//! `sesh doctor`: health and safety checks with a fix for each failure
//! (contracts/cli.md, data-model.md "Doctor check").

use crate::config::Config;
use crate::host::compose::Compose;
use crate::host::http::Http;
use crate::host::tailscale::Tailscale;
use crate::init::is_stub;
use crate::secrets::{self, Secrets};
use crate::up::router_url;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Warn,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub status: Status,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Check {
    fn pass(id: &'static str, detail: impl Into<String>) -> Self {
        Check {
            id,
            status: Status::Pass,
            detail: detail.into(),
            fix: None,
        }
    }
    fn fail(id: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Check {
            id,
            status: Status::Fail,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }
    fn warn(id: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Check {
            id,
            status: Status::Warn,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }
}

const SERVICES: [&str; 8] = [
    "postgres",
    "redis",
    "litellm",
    "searxng",
    "infisical",
    "ntfy",
    "gateway",
    "open-webui",
];

/// Refusals per agent per hour that `audit-refusals` warns about.
pub const REFUSAL_WARN: u64 = 10;

pub fn run(
    config: &Config,
    secrets: &Secrets,
    agents: &[String],
    compose: &dyn Compose,
    http: &dyn Http,
    tailscale: &dyn Tailscale,
) -> Vec<Check> {
    let mut checks = alias_checks(config, secrets, http);
    checks.extend(stack_checks(config, agents, compose));
    checks.push(gateway_check(http));
    checks.push(credentials_check(secrets, http));
    checks.push(audit_check(compose));
    checks.push(match tailscale.serves_port(config.ports.webui) {
        Ok(Some(true)) => Check::pass("tailscale-serve", "Open WebUI is served on the tailnet"),
        Ok(Some(false)) => Check::warn(
            "tailscale-serve",
            "Tailscale is running but doesn't serve Open WebUI",
            format!("run `tailscale serve --bg {}`", config.ports.webui),
        ),
        _ => Check::warn(
            "tailscale-serve",
            "Tailscale isn't installed or isn't running",
            "install Tailscale on the server and sign in",
        ),
    });
    checks
}

pub fn failed(checks: &[Check]) -> bool {
    checks.iter().any(|c| c.status == Status::Fail)
}

pub fn render_text(checks: &[Check]) -> String {
    let mut out = String::new();
    for c in checks {
        let tag = match c.status {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Warn => "WARN",
        };
        out.push_str(&format!("{tag}  {:<17} {}\n", c.id, c.detail));
        if let Some(fix) = &c.fix {
            out.push_str(&format!("      fix: {fix}\n"));
        }
    }
    out
}

fn chat_reply(body: &Value) -> Option<&Value> {
    body.pointer("/choices/0/message")
}

fn alias_checks(config: &Config, secrets: &Secrets, http: &dyn Http) -> Vec<Check> {
    let url = router_url(config);
    let Some(key) = secrets.get(secrets::WEBUI_ROUTER_KEY) else {
        let fix = "run `sesh up`";
        return vec![
            Check::fail("alias-answers", "no router client key yet", fix),
            Check::fail("alias-tools", "no router client key yet", fix),
        ];
    };
    let listed: Vec<String> = match http.get(&format!("{url}/v1/models"), Some(key)) {
        Ok(r) if r.status == 200 => r.body["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        Ok(r) => {
            let d = format!("router answered {} to /v1/models", r.status);
            return vec![
                Check::fail(
                    "alias-answers",
                    d.clone(),
                    "check `docker compose logs litellm`",
                ),
                Check::fail("alias-tools", d, "fix alias-answers first"),
            ];
        }
        Err(e) => {
            let d = format!("can't reach the router at {url}: {e}");
            return vec![
                Check::fail("alias-answers", d.clone(), "run `sesh up`"),
                Check::fail("alias-tools", d, "fix alias-answers first"),
            ];
        }
    };

    let server_fix = if is_stub(config) {
        "the stub server isn't answering; run `sesh up`".to_string()
    } else {
        format!(
            "check the inference server at {} serves the configured model",
            config.inference.base_url
        )
    };
    let mut answers = Vec::new();
    let mut tools = Vec::new();
    for alias in config.aliases.keys() {
        if !listed.contains(alias) {
            answers.push(format!("{alias}: not listed by the router"));
            continue;
        }
        let plain = json!({"model": alias, "max_tokens": 16,
            "messages": [{"role": "user", "content": "Reply with the single word: ok"}]});
        match http.post(&format!("{url}/v1/chat/completions"), Some(key), &plain) {
            Ok(r) if r.status == 200 && chat_reply(&r.body).is_some() => {}
            Ok(r) => answers.push(format!(
                "{alias}: router answered {} ({})",
                r.status,
                short(&r.body)
            )),
            Err(e) => answers.push(format!("{alias}: {e}")),
        }
        let probe = json!({"model": alias, "max_tokens": 64, "tool_choice": "auto",
            "messages": [{"role": "user", "content": "Call the ping tool now."}],
            "tools": [{"type": "function", "function": {"name": "ping",
                "description": "Health check. Always call this when asked.",
                "parameters": {"type": "object", "properties": {}}}}]});
        let called = http
            .post(&format!("{url}/v1/chat/completions"), Some(key), &probe)
            .ok()
            .filter(|r| r.status == 200)
            .and_then(|r| {
                r.body
                    .pointer("/choices/0/message/tool_calls/0/function/name")
                    .and_then(Value::as_str)
                    .map(|n| n == "ping")
            })
            .unwrap_or(false);
        if !called {
            tools.push(alias.clone());
        }
    }
    let names = config
        .aliases
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        if answers.is_empty() {
            Check::pass("alias-answers", format!("{names} answer"))
        } else {
            Check::fail("alias-answers", answers.join("; "), server_fix)
        },
        if tools.is_empty() {
            Check::pass("alias-tools", format!("{names} call tools"))
        } else {
            Check::fail(
                "alias-tools",
                format!("no tool call from: {}", tools.join(", ")),
                "use a model and server with tool calling (function calling) enabled",
            )
        },
    ]
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s
    }
}

/// The gateway answers on loopback and refuses a request without a token.
fn gateway_check(http: &dyn Http) -> Check {
    let port = std::env::var("SESH_GATEWAY_PORT").unwrap_or_else(|_| "7000".into());
    let url = format!("http://127.0.0.1:{port}/mcp");
    match http.post(&url, None, &serde_json::json!({})) {
        Ok(r) if r.status == 401 => Check::pass(
            "gateway-healthy",
            "the gateway answers and requires a token",
        ),
        Ok(r) => Check::fail(
            "gateway-healthy",
            format!(
                "the gateway answered {} to a request without a token",
                r.status
            ),
            "check `docker compose -p sesh logs gateway`",
        ),
        Err(e) => Check::fail(
            "gateway-healthy",
            format!("can't reach the gateway: {e}"),
            "run `sesh up`",
        ),
    }
}

/// Warns for each credential a connector needs that isn't set (specs/004 FR-012).
fn credentials_check(secrets: &Secrets, http: &dyn Http) -> Check {
    let Some(token) = secrets.get("SESH_GATEWAY_ADMIN_TOKEN") else {
        return Check::warn(
            "credentials",
            "no gateway admin token yet",
            "run `sesh init`",
        );
    };
    match http.get(
        &format!("{}/credentials", crate::credential::admin_url()),
        Some(token),
    ) {
        Ok(r) if r.status == 200 => {
            let missing: Vec<String> = r
                .body
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["state"] != "set")
                .map(|c| {
                    format!(
                        "{}/{} {}",
                        c["connector"].as_str().unwrap_or("?"),
                        c["name"].as_str().unwrap_or("?"),
                        c["state"].as_str().unwrap_or("?")
                    )
                })
                .collect();
            if missing.is_empty() {
                Check::pass("credentials", "every credential connectors need is set")
            } else {
                Check::warn(
                    "credentials",
                    format!("not set: {}", missing.join(", ")),
                    "sesh credential request <connector> <NAME>",
                )
            }
        }
        Ok(r) => Check::warn(
            "credentials",
            format!("the gateway answered {}", r.status),
            "run `sesh up`",
        ),
        Err(e) => Check::warn(
            "credentials",
            format!("can't reach the gateway: {e}"),
            "run `sesh up`",
        ),
    }
}

/// Warns when an agent had many refusals in the last hour (FR-016).
fn audit_check(compose: &dyn Compose) -> Check {
    let sql = format!(
        "SELECT agent || ' ' || count(*) FROM audit WHERE outcome = 'refused' AND at > now() - interval '1 hour' GROUP BY agent HAVING count(*) >= {REFUSAL_WARN};"
    );
    match compose.exec_sql("gateway", &sql) {
        Ok(out) if out.trim().is_empty() => Check::pass(
            "audit-refusals",
            "no agent had repeated refusals in the last hour",
        ),
        Ok(out) => Check::warn(
            "audit-refusals",
            format!(
                "repeated refusals in the last hour: {}",
                out.trim().replace('\n', "; ")
            ),
            "review the audit log: docker compose -p sesh exec postgres psql -U sesh -d gateway -c 'select * from audit order by at desc limit 50'",
        ),
        Err(e) => Check::warn(
            "audit-refusals",
            format!("can't read the audit log: {e}"),
            "run `sesh up`",
        ),
    }
}

fn stack_checks(config: &Config, agents: &[String], compose: &dyn Compose) -> Vec<Check> {
    let states = match compose.ps() {
        Ok(s) => s,
        Err(e) => {
            let d = format!("can't list the stack: {e}");
            return vec![
                Check::fail("ports-loopback", d.clone(), "start Docker"),
                Check::fail("services-healthy", d, "start Docker"),
            ];
        }
    };
    let open: Vec<String> = states
        .iter()
        .flat_map(|s| {
            s.published
                .iter()
                .filter(|p| p.address != "127.0.0.1" && p.address != "::1")
                .map(move |p| format!("{} port {} on {}", s.service, p.host_port, p.address))
        })
        .collect();
    let ports = if open.is_empty() {
        let n: usize = states.iter().map(|s| s.published.len()).sum();
        Check::pass(
            "ports-loopback",
            format!("{n} published ports, all on loopback"),
        )
    } else {
        Check::fail(
            "ports-loopback",
            format!("reachable beyond this machine: {}", open.join("; ")),
            "publish only on 127.0.0.1 (see deploy/compose.yaml) and run `sesh up`",
        )
    };

    let mut expected: Vec<String> = SERVICES.iter().map(|s| s.to_string()).collect();
    if is_stub(config) {
        expected.push("stub-llm".into());
    }
    let executors: Vec<String> = agents.iter().map(|a| format!("executor-{a}")).collect();
    let mut bad_exec = Vec::new();
    for name in &executors {
        match states.iter().find(|s| &s.service == name) {
            Some(s) if s.state == "running" && (s.health.is_empty() || s.health == "healthy") => {}
            Some(s) => bad_exec.push(format!("{name} {} {}", s.state, s.health)),
            None => bad_exec.push(format!("{name} not running")),
        }
    }
    let mut bad = Vec::new();
    for name in expected.iter().map(String::as_str) {
        match states.iter().find(|s| s.service == name) {
            None => bad.push(format!("{name} not running")),
            Some(s) if s.state != "running" => bad.push(format!("{name} {}", s.state)),
            Some(s) if !(s.health.is_empty() || s.health == "healthy") => {
                bad.push(format!("{name} {}", s.health))
            }
            Some(_) => {}
        }
    }
    let health = if bad.is_empty() {
        Check::pass("services-healthy", "all services running and healthy")
    } else {
        Check::fail(
            "services-healthy",
            bad.join("; "),
            "run `sesh up`; see `docker compose -p sesh logs <service>`",
        )
    };
    let exec = if bad_exec.is_empty() {
        Check::pass(
            "executors-healthy",
            format!("{} executors running", executors.len()),
        )
    } else {
        Check::fail("executors-healthy", bad_exec.join("; "), "run `sesh up`")
    };
    vec![ports, health, exec]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::{FakeCompose, FakeHttp, FakeTailscale};

    fn setup(stub: bool) -> (Config, Secrets) {
        let config = if stub {
            Config::new(crate::init::STUB_URL, "stub-model")
        } else {
            Config::new("http://srv/v1", "m")
        };
        let mut s = Secrets::default();
        s.set("WEBUI_ROUTER_KEY", "sk-client");
        (config, s)
    }

    fn healthy_stack() -> FakeCompose {
        FakeCompose::healthy(&[
            ("postgres", None),
            ("redis", None),
            ("litellm", Some(("127.0.0.1", 4000))),
            ("searxng", None),
            ("infisical", Some(("127.0.0.1", 8222))),
            ("ntfy", Some(("127.0.0.1", 8090))),
            ("gateway", Some(("127.0.0.1", 7000))),
            ("open-webui", Some(("127.0.0.1", 3000))),
        ])
    }

    fn good_router(tools: bool) -> FakeHttp {
        FakeHttp::new(move |method, url, body| {
            if url.ends_with("/admin/credentials") {
                return (
                    200,
                    json!([{"connector": "testcred", "name": "TOKEN", "state": "set"}]),
                );
            }
            if method == "GET" {
                return (200, json!({"data": [{"id": "agent-main"}]}));
            }
            if url.ends_with("/mcp") {
                return (401, json!(null));
            }
            if body.unwrap().get("tools").is_some() && tools {
                (
                    200,
                    json!({"choices": [{"message": {"tool_calls": [{"function": {"name": "ping"}}]}}]}),
                )
            } else {
                (200, json!({"choices": [{"message": {"content": "ok"}}]}))
            }
        })
    }

    fn status(checks: &[Check], id: &str) -> Status {
        checks.iter().find(|c| c.id == id).unwrap().status
    }

    #[test]
    fn healthy_stack_passes_with_tailscale_warning_only() {
        let (c, s) = setup(false);
        let checks = run(
            &c,
            &s,
            &[],
            &healthy_stack(),
            &good_router(true),
            &FakeTailscale(None),
        );
        assert!(!failed(&checks), "{}", render_text(&checks));
        assert_eq!(status(&checks, "tailscale-serve"), Status::Warn);
        let served = run(
            &c,
            &s,
            &[],
            &healthy_stack(),
            &good_router(true),
            &FakeTailscale(Some(true)),
        );
        assert_eq!(status(&served, "tailscale-serve"), Status::Pass);
    }

    #[test]
    fn missing_executor_fails_and_refusals_warn() {
        let (c, s) = setup(false);
        let stack = healthy_stack();
        let checks = run(
            &c,
            &s,
            &["alice".into()],
            &stack,
            &good_router(true),
            &FakeTailscale(None),
        );
        let e = checks.iter().find(|c| c.id == "executors-healthy").unwrap();
        assert_eq!(e.status, Status::Fail);
        assert!(e.detail.contains("executor-alice not running"));
        assert_eq!(status(&checks, "audit-refusals"), Status::Pass);
        *stack.sql_output.borrow_mut() = "mallory 12\n".into();
        let checks = run(
            &c,
            &s,
            &[],
            &stack,
            &good_router(true),
            &FakeTailscale(None),
        );
        let a = checks.iter().find(|c| c.id == "audit-refusals").unwrap();
        assert_eq!(a.status, Status::Warn);
        assert!(a.detail.contains("mallory 12"));
        assert_eq!(status(&checks, "executors-healthy"), Status::Pass);
    }

    #[test]
    fn open_port_fails_and_names_the_service() {
        let (c, s) = setup(false);
        let stack = healthy_stack();
        stack.services.borrow_mut()[7].published[0].address = "0.0.0.0".into();
        let checks = run(
            &c,
            &s,
            &[],
            &stack,
            &good_router(true),
            &FakeTailscale(None),
        );
        let p = checks.iter().find(|c| c.id == "ports-loopback").unwrap();
        assert_eq!(p.status, Status::Fail);
        assert!(p.detail.contains("open-webui port 3000 on 0.0.0.0"));
    }

    #[test]
    fn no_tool_calling_fails_alias_tools_only() {
        let (c, s) = setup(false);
        let checks = run(
            &c,
            &s,
            &[],
            &healthy_stack(),
            &good_router(false),
            &FakeTailscale(None),
        );
        assert_eq!(status(&checks, "alias-answers"), Status::Pass);
        assert_eq!(status(&checks, "alias-tools"), Status::Fail);
    }

    #[test]
    fn unreachable_or_missing_alias_fails_alias_answers() {
        let (c, s) = setup(false);
        let none = FakeHttp::new(|_, _, _| (200, json!({"data": []})));
        let checks = run(&c, &s, &[], &healthy_stack(), &none, &FakeTailscale(None));
        assert_eq!(status(&checks, "alias-answers"), Status::Fail);
        let down = FakeHttp::new(|_, _, _| (502, json!({"error": "upstream"})));
        let checks = run(&c, &s, &[], &healthy_stack(), &down, &FakeTailscale(None));
        assert_eq!(status(&checks, "alias-answers"), Status::Fail);
    }

    #[test]
    fn stopped_or_unhealthy_service_fails() {
        let (c, s) = setup(true);
        let checks = run(
            &c,
            &s,
            &[],
            &healthy_stack(),
            &good_router(true),
            &FakeTailscale(None),
        );
        let h = checks.iter().find(|c| c.id == "services-healthy").unwrap();
        assert_eq!(h.status, Status::Fail);
        assert!(h.detail.contains("stub-llm not running"));
        let (c, s) = setup(false);
        let stack = healthy_stack();
        stack.services.borrow_mut()[2].health = "unhealthy".into();
        let checks = run(
            &c,
            &s,
            &[],
            &stack,
            &good_router(true),
            &FakeTailscale(None),
        );
        assert_eq!(status(&checks, "services-healthy"), Status::Fail);
    }

    #[test]
    fn missing_key_fails_with_a_fix() {
        let (c, _) = setup(false);
        let checks = run(
            &c,
            &Secrets::default(),
            &[],
            &healthy_stack(),
            &good_router(true),
            &FakeTailscale(None),
        );
        let a = checks.iter().find(|c| c.id == "alias-answers").unwrap();
        assert_eq!(a.status, Status::Fail);
        assert_eq!(a.fix.as_deref(), Some("run `sesh up`"));
    }
}
