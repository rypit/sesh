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

const SERVICES: [&str; 12] = [
    "postgres",
    // Each service's own Redis (specs/007 research.md R3).
    "redis-openwebui",
    "redis-infisical",
    "redis-paperless",
    "litellm",
    "searxng",
    "infisical",
    "ntfy",
    "paperless",
    "miniflux",
    "gateway",
    "open-webui",
];

/// Refusals per agent per hour that `audit-refusals` warns about.
pub const REFUSAL_WARN: u64 = 10;

#[cfg(test)]
pub fn run(
    config: &Config,
    secrets: &Secrets,
    agents: &[String],
    compose: &dyn Compose,
    http: &dyn Http,
    tailscale: &dyn Tailscale,
) -> Vec<Check> {
    run_with(config, secrets, agents, &[], &[], compose, http, tailscale)
}

#[allow(clippy::too_many_arguments)]
/// [`run`] for a stack with agent runtimes (`runtimes`: their agents'
/// names; specs/009).
pub fn run_with(
    config: &Config,
    secrets: &Secrets,
    agents: &[String],
    runtimes: &[String],
    browsers: &[String],
    compose: &dyn Compose,
    http: &dyn Http,
    tailscale: &dyn Tailscale,
) -> Vec<Check> {
    let mut checks = alias_checks(config, secrets, http);
    checks.extend(stack_checks(config, agents, compose));
    checks.push(gateway_check(http));
    checks.push(credentials_check(secrets, http));
    checks.push(google_check(secrets, http));
    checks.push(paperless_check(secrets, http));
    checks.push(feeds_check(secrets, http));
    checks.push(audit_check(compose));
    checks.push(isolation_check(compose));
    checks.push(network_check_with(config, compose, runtimes, browsers));
    if !runtimes.is_empty() {
        checks.push(runtime_check(runtimes, compose));
    }
    if !browsers.is_empty() {
        checks.push(browser_check(browsers, compose));
    }
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

/// specs/005 contracts/cli.md: each owner's Google connection. Missing is
/// fine (not everyone uses Google); revoked needs a new consent.
fn google_check(secrets: &Secrets, http: &dyn Http) -> Check {
    let Some(token) = secrets.get("SESH_GATEWAY_ADMIN_TOKEN") else {
        return Check::warn("google", "no gateway admin token yet", "run `sesh init`");
    };
    match http.get(
        &format!("{}/google/status", crate::credential::admin_url()),
        Some(token),
    ) {
        Ok(r) if r.status == 200 => {
            let all: Vec<(String, String)> = r
                .body
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    (
                        c["owner"].as_str().unwrap_or("?").to_string(),
                        c["state"].as_str().unwrap_or("?").to_string(),
                    )
                })
                .collect();
            let revoked: Vec<&str> = all
                .iter()
                .filter(|(_, s)| s == "revoked")
                .map(|(o, _)| o.as_str())
                .collect();
            let connected: Vec<&str> = all
                .iter()
                .filter(|(_, s)| s == "connected")
                .map(|(o, _)| o.as_str())
                .collect();
            if !revoked.is_empty() {
                Check::warn(
                    "google",
                    format!("Google access was revoked for: {}", revoked.join(", ")),
                    "sesh google connect <user>",
                )
            } else if connected.is_empty() {
                Check::pass("google", "no one has connected Google")
            } else {
                Check::pass("google", format!("connected: {}", connected.join(", ")))
            }
        }
        Ok(r) => Check::warn(
            "google",
            format!("the gateway answered {}", r.status),
            "run `sesh up`",
        ),
        Err(e) => Check::warn(
            "google",
            format!("can't reach the gateway: {e}"),
            "run `sesh up`",
        ),
    }
}

fn paperless_check(secrets: &Secrets, http: &dyn Http) -> Check {
    let Some(token) = secrets.get("SESH_GATEWAY_ADMIN_TOKEN") else {
        return Check::warn("paperless", "no gateway admin token yet", "run `sesh init`");
    };
    match http.get(
        &format!("{}/paperless", crate::credential::admin_url()),
        Some(token),
    ) {
        Ok(r) if r.status == 200 && r.body["state"] == "ok" => {
            Check::pass("paperless", "the gateway can reach the document service")
        }
        Ok(r) if r.status == 200 => Check::warn(
            "paperless",
            format!(
                "the document service: {}",
                r.body["message"].as_str().unwrap_or("unavailable")
            ),
            "check `docker compose logs paperless`, then `sesh up`",
        ),
        Ok(r) => Check::warn(
            "paperless",
            format!("the gateway answered {}", r.status),
            "run `sesh up`",
        ),
        Err(e) => Check::warn(
            "paperless",
            format!("can't reach the gateway: {e}"),
            "run `sesh up`",
        ),
    }
}

/// specs/006 contracts/cli.md: feeds per owner, from Miniflux through the
/// gateway; warns when Miniflux is unreachable or a feed's last update
/// failed (Miniflux is not a safety component, like Paperless).
fn feeds_check(secrets: &Secrets, http: &dyn Http) -> Check {
    let Some(token) = secrets.get("SESH_GATEWAY_ADMIN_TOKEN") else {
        return Check::warn("feeds", "no gateway admin token yet", "run `sesh init`");
    };
    let r = match http.get(
        &format!("{}/feeds", crate::credential::admin_url()),
        Some(token),
    ) {
        Ok(r) if r.status == 200 => r,
        Ok(r) => {
            return Check::warn(
                "feeds",
                format!("the gateway answered {}", r.status),
                "run `sesh up`",
            );
        }
        Err(e) => {
            return Check::warn(
                "feeds",
                format!("can't reach the gateway: {e}"),
                "run `sesh up`",
            );
        }
    };
    if r.body["state"] != "ok" {
        return Check::warn(
            "feeds",
            format!(
                "the feed service: {}",
                r.body["message"].as_str().unwrap_or("unavailable")
            ),
            "check `docker compose logs miniflux`, then `sesh up`",
        );
    }
    let feeds: Vec<&Value> = r.body["feeds"].as_array().into_iter().flatten().collect();
    let mut per_owner: std::collections::BTreeMap<&str, usize> = Default::default();
    for f in &feeds {
        *per_owner
            .entry(f["owner"].as_str().unwrap_or("?"))
            .or_default() += 1;
    }
    let failed: Vec<String> = feeds
        .iter()
        .filter_map(|f| {
            f["error"].as_str().map(|e| {
                format!(
                    "{}/{} ({e})",
                    f["owner"].as_str().unwrap_or("?"),
                    f["name"].as_str().unwrap_or("?")
                )
            })
        })
        .collect();
    if !failed.is_empty() {
        return Check::warn(
            "feeds",
            format!("last update failed: {}", failed.join(", ")),
            "check the address, or `sesh feed remove <owner> <name>`",
        );
    }
    if per_owner.is_empty() {
        return Check::pass("feeds", "no feeds yet");
    }
    let counts: Vec<String> = per_owner.iter().map(|(o, n)| format!("{o} {n}")).collect();
    Check::pass("feeds", format!("feeds: {}", counts.join(", ")))
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

/// The `db-isolation` query (specs/007 research.md R6): one line per
/// problem, nothing when every service logs in as its own role.
/// `sesh up`, doctor and the operator use the superuser over the Postgres
/// container's local socket, which has no client address; a superuser
/// session with one comes from another container.
fn isolation_sql() -> String {
    let roles: Vec<String> = crate::up::SERVICE_DATABASES
        .iter()
        .map(|(role, _)| format!("('{role}')"))
        .collect();
    let names: Vec<String> = crate::up::SERVICE_DATABASES
        .iter()
        .map(|(role, _)| format!("'{role}'"))
        .collect();
    format!(
        "SELECT DISTINCT 'superuser session from ' || host(a.client_addr) || ' (' || \
           coalesce(nullif(a.application_name, ''), 'no application name') || ')' \
         FROM pg_stat_activity a JOIN pg_roles r ON r.oid = a.usesysid \
         WHERE r.rolsuper AND a.client_addr IS NOT NULL \
         UNION ALL \
         SELECT 'database ' || e.name || CASE WHEN d.oid IS NULL THEN ' is missing' \
           ELSE ' is owned by ' || pg_get_userbyid(d.datdba) END \
         FROM (VALUES {}) AS e(name) LEFT JOIN pg_database d ON d.datname = e.name \
         WHERE d.oid IS NULL OR pg_get_userbyid(d.datdba) <> e.name \
         UNION ALL \
         SELECT 'role ' || rolname || ' has administrative rights' FROM pg_roles \
         WHERE rolname IN ({}) \
           AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls);",
        roles.join(", "),
        names.join(", ")
    )
}

/// specs/007 FR-010: no service uses the superuser; each service database
/// belongs to its own login, which has no administrative rights.
/// The query for Postgres's active login rules, in file order.
pub const HBA_RULES_SQL: &str = "SELECT type, coalesce(user_name[1], ''), coalesce(address, ''), coalesce(netmask, ''), auth_method FROM pg_hba_file_rules ORDER BY line_number";

/// The prefix length of a netmask (`255.255.0.0` → 16; IPv6 too).
fn prefix_len(netmask: &str) -> Option<u32> {
    match netmask.parse::<std::net::IpAddr>().ok()? {
        std::net::IpAddr::V4(m) => Some(u32::from(m).count_ones()),
        std::net::IpAddr::V6(m) => Some(u128::from(m).count_ones()),
    }
}

/// specs/010: each agent's browser is running. Chromium exits at once
/// when it can't build its sandbox, so a browser that keeps restarting
/// almost always means the host forbids unprivileged user namespaces.
fn browser_check(browsers: &[String], compose: &dyn Compose) -> Check {
    const ID: &str = "browsers";
    let states = compose.ps().unwrap_or_default();
    let bad: Vec<String> = browsers
        .iter()
        .filter_map(|a| {
            let svc = format!("browser-{a}");
            match states.iter().find(|s| s.service == svc) {
                Some(s) if s.state == "running" => None,
                Some(s) => Some(format!("{svc} is {}", s.state)),
                None => Some(format!("{svc} isn't running")),
            }
        })
        .collect();
    if bad.is_empty() {
        Check::pass(
            ID,
            format!(
                "{} browser(s) running, Chromium's sandbox on",
                browsers.len()
            ),
        )
    } else {
        Check::fail(
            ID,
            bad.join("; "),
            "if its log says \"No usable sandbox\", the host forbids unprivileged user namespaces: on Linux run `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` (and set it in /etc/sysctl.d), then `sesh up`",
        )
    }
}

/// specs/009 contracts/cli.md: each agent runtime is running and healthy,
/// has no built-in toolset enabled, and runs the configuration sesh wrote.
fn runtime_check(runtimes: &[String], compose: &dyn Compose) -> Check {
    const ID: &str = "agent-runtimes";
    let states = compose.ps().unwrap_or_default();
    let mut problems = vec![];
    for a in runtimes {
        let svc = crate::hermes::service(a);
        match states.iter().find(|s| s.service == svc) {
            Some(s) if s.state == "running" && s.health == "healthy" => {}
            Some(s) => {
                problems.push(format!("{a} is {} {}", s.state, s.health));
                continue;
            }
            None => {
                problems.push(format!("{a} isn't running"));
                continue;
            }
        }
        // The key stays inside the container: it's expanded there.
        let toolsets = compose.exec(
            &svc,
            &[
                "sh",
                "-c",
                "curl -fsS -H \"Authorization: Bearer $API_SERVER_KEY\" http://127.0.0.1:8642/v1/toolsets",
            ],
        );
        match toolsets
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        {
            Some(v) => {
                let on: Vec<String> = v["data"]
                    .as_array()
                    .map(|d| {
                        d.iter()
                            .filter(|t| t["enabled"] == true)
                            .filter_map(|t| t["name"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if !on.is_empty() {
                    problems.push(format!(
                        "{a} has built-in toolsets enabled: {}",
                        on.join(", ")
                    ));
                }
            }
            None => problems.push(format!("can't read {a}'s toolsets")),
        }
        match compose.exec(&svc, &["cat", "/opt/data/config.yaml"]) {
            Ok(c) if c == crate::hermes::config_text(a) => {}
            Ok(_) => problems.push(format!("{a}'s config.yaml isn't the one sesh writes")),
            Err(_) => problems.push(format!("can't read {a}'s config.yaml")),
        }
    }
    if problems.is_empty() {
        Check::pass(
            ID,
            format!(
                "{} agent runtime(s) healthy, tools: the gateway's only",
                runtimes.len()
            ),
        )
    } else {
        Check::fail(
            ID,
            problems.join("; "),
            "run `sesh up`; if it persists, see `docker compose logs` for the runtime",
        )
    }
}

/// specs/008 contracts/cli.md: each running service is on exactly its
/// networks, the old shared network is gone, and Postgres's active login
/// rules bind each login to its own network's current ranges.
#[cfg(test)]
fn network_check(config: &Config, compose: &dyn Compose) -> Check {
    network_check_with(config, compose, &[], &[])
}

fn network_check_with(
    config: &Config,
    compose: &dyn Compose,
    runtimes: &[String],
    browsers: &[String],
) -> Check {
    let stub = crate::init::is_stub(config);
    let runtimes: Vec<&str> = runtimes.iter().map(String::as_str).collect();
    let browsers: Vec<&str> = browsers.iter().map(String::as_str).collect();
    let install = crate::layout::Install {
        runtimes: &runtimes,
        browsers: &browsers,
    };
    let mut problems = vec![];
    match compose.service_networks() {
        Ok(running) => {
            for (svc, nets) in running {
                if svc.starts_with("executor-") || svc == "kiwix" {
                    continue;
                }
                let want = crate::layout::networks_of_with(&svc, stub, &install);
                if want.is_empty() {
                    if svc.starts_with("hermes-") {
                        problems.push(format!("{svc} runs but its agent has no runtime"));
                    }
                    if svc.starts_with("browser-") {
                        problems.push(format!("{svc} runs but its agent has no browser"));
                    }
                    continue;
                }
                for n in &nets {
                    let exec = svc == "gateway" && n.starts_with("exec-");
                    if !exec && !want.contains(n) {
                        problems.push(format!("{svc} is on {n} (expected {})", want.join(", ")));
                    }
                }
                for w in &want {
                    if !nets.iter().any(|n| n == w) {
                        problems.push(format!("{svc} isn't on {w}"));
                    }
                }
            }
        }
        Err(e) => problems.push(format!("can't read the containers' networks: {e}")),
    }
    if compose
        .network_subnets("default")
        .is_ok_and(|s| !s.is_empty())
    {
        problems.push("sesh_default still exists".into());
    }
    match compose.exec_sql("sesh", HBA_RULES_SQL) {
        Ok(out) => {
            let rows: Vec<Vec<&str>> = out
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.split('|').collect())
                .collect();
            let mut allowed = std::collections::BTreeSet::new();
            for r in &rows {
                if r.len() == 5 && r[0] == "host" && r[4] != "reject" {
                    match prefix_len(r[3]) {
                        Some(p) => {
                            allowed.insert((r[1].to_string(), format!("{}/{p}", r[2])));
                        }
                        None => {
                            problems.push(format!("a login rule for {} has no address range", r[1]))
                        }
                    }
                }
            }
            let mut want = std::collections::BTreeSet::new();
            for (role, _, net) in crate::layout::DB_LOGINS {
                for s in compose.network_subnets(net).unwrap_or_default() {
                    want.insert((role.to_string(), s));
                }
            }
            for (role, range) in allowed.difference(&want) {
                problems.push(format!("a database login rule lets {role} in from {range}"));
            }
            for (role, range) in want.difference(&allowed) {
                problems.push(format!("no database login rule for {role} on {range}"));
            }
            if !rows.last().is_some_and(|r| {
                r.len() == 5 && r[0] == "host" && r[1] == "all" && r[4] == "reject"
            }) {
                problems
                    .push("the database login rules don't end by rejecting everything else".into());
            }
        }
        Err(e) => problems.push(format!("can't read the database login rules: {e}")),
    }
    if problems.is_empty() {
        Check::pass(
            "network-layout",
            "each service is on only its own networks; each database login only from its own",
        )
    } else {
        Check::fail("network-layout", problems.join("; "), "run `sesh up`")
    }
}

fn isolation_check(compose: &dyn Compose) -> Check {
    match compose.exec_sql("sesh", &isolation_sql()) {
        Ok(out) if out.trim().is_empty() => Check::pass(
            "db-isolation",
            "every service logs in as its own role, owning only its database",
        ),
        Ok(out) => Check::fail(
            "db-isolation",
            out.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("; "),
            "run `sesh up`",
        ),
        Err(e) => Check::fail(
            "db-isolation",
            format!("can't check the database logins: {e}"),
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
        let f = FakeCompose::healthy(&[
            ("postgres", None),
            ("redis-openwebui", None),
            ("litellm", Some(("127.0.0.1", 4000))),
            ("searxng", None),
            ("infisical", Some(("127.0.0.1", 8222))),
            ("ntfy", Some(("127.0.0.1", 8090))),
            ("paperless", None),
            ("miniflux", None),
            ("gateway", Some(("127.0.0.1", 7000))),
            ("open-webui", Some(("127.0.0.1", 3000))),
            ("redis-infisical", None),
            ("redis-paperless", None),
        ]);
        with_layout(&f);
        f
    }

    /// The intended layout (specs/008): networks per the table, a range per
    /// database network, and matching login rules.
    fn with_layout(f: &FakeCompose) {
        *f.service_nets.borrow_mut() = crate::layout::services(false)
            .into_iter()
            .map(|s| {
                (
                    s.to_string(),
                    crate::layout::networks_of(s, false)
                        .iter()
                        .map(|n| n.to_string())
                        .collect(),
                )
            })
            .collect();
        let mut rules = String::from("local|all|||trust\n");
        for (i, (role, _, net)) in crate::layout::DB_LOGINS.iter().enumerate() {
            f.subnets
                .borrow_mut()
                .insert(net.to_string(), vec![format!("172.{}.0.0/16", 40 + i)]);
            rules.push_str(&format!(
                "host|{role}|172.{}.0.0|255.255.0.0|scram-sha-256\n",
                40 + i
            ));
        }
        rules.push_str("host|all|all|0.0.0.0|reject\n");
        f.sql_by_query
            .borrow_mut()
            .push(("pg_hba_file_rules".into(), rules));
    }

    #[test]
    fn network_layout_names_each_drift() {
        let (c, _) = setup(false);
        let f = healthy_stack();
        assert_eq!(
            network_check(&c, &f).status,
            Status::Pass,
            "{}",
            network_check(&c, &f).detail
        );
        // A service on another service's link.
        f.service_nets
            .borrow_mut()
            .iter_mut()
            .find(|(s, _)| s == "open-webui")
            .unwrap()
            .1
            .push("secrets".into());
        let r = network_check(&c, &f);
        assert_eq!(r.status, Status::Fail);
        assert!(
            r.detail.contains("open-webui is on secrets"),
            "{}",
            r.detail
        );
        // The old shared network, and a login rule wider than its network.
        let f = healthy_stack();
        f.subnets
            .borrow_mut()
            .insert("default".into(), vec!["172.18.0.0/16".into()]);
        f.sql_by_query.borrow_mut()[0].1 =
            "local|all|||trust\nhost|litellm|0.0.0.0|0.0.0.0|scram-sha-256\n".into();
        let r = network_check(&c, &f).detail;
        assert!(r.contains("sesh_default still exists"), "{r}");
        assert!(r.contains("lets litellm in from 0.0.0.0/0"), "{r}");
        assert!(r.contains("no database login rule for paperless"), "{r}");
        assert!(r.contains("don't end by rejecting"), "{r}");
    }

    #[test]
    fn netmasks_become_prefix_lengths() {
        assert_eq!(prefix_len("255.255.0.0"), Some(16));
        assert_eq!(prefix_len("255.255.255.240"), Some(28));
        assert_eq!(prefix_len("ffff:ffff:ffff:ffff::"), Some(64));
        assert_eq!(prefix_len("nope"), None);
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
        stack.services.borrow_mut()[9].published[0].address = "0.0.0.0".into();
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

    #[test]
    fn feeds_counts_per_owner_and_warns_on_failures() {
        let mut s = Secrets::default();
        s.set("SESH_GATEWAY_ADMIN_TOKEN", "admin");
        let reply = |body: Value| {
            FakeHttp::new(move |_, url, _| {
                assert!(url.ends_with("/admin/feeds"));
                (200, body.clone())
            })
        };
        let ok = json!({"state": "ok", "feeds": [
            {"owner": "ana", "name": "a", "error": null},
            {"owner": "ana", "name": "b", "error": null},
            {"owner": "household", "name": "h", "error": null}
        ]});
        let c = feeds_check(&s, &reply(ok));
        assert_eq!(c.status, Status::Pass);
        assert_eq!(c.detail, "feeds: ana 2, household 1");
        let bad = json!({"state": "ok", "feeds": [{"owner": "ana", "name": "big", "error": "the feed answered 500"}]});
        let c = feeds_check(&s, &reply(bad));
        assert_eq!(c.status, Status::Warn);
        assert!(
            c.detail.contains("ana/big (the feed answered 500)"),
            "{}",
            c.detail
        );
        let down = json!({"state": "unavailable", "message": "the feed service is unavailable"});
        let c = feeds_check(&s, &reply(down));
        assert_eq!(c.status, Status::Warn);
        let none = json!({"state": "ok", "feeds": []});
        assert_eq!(feeds_check(&s, &reply(none)).detail, "no feeds yet");
    }

    #[test]
    fn db_isolation_passes_with_no_problems_and_names_each_one() {
        let ok = FakeCompose::healthy(&[]);
        let c = isolation_check(&ok);
        assert_eq!(c.status, Status::Pass, "{}", c.detail);
        assert_eq!(
            *ok.calls.borrow(),
            vec!["sql sesh"],
            "asked as the superuser"
        );
        let bad = FakeCompose::healthy(&[]);
        *bad.sql_output.borrow_mut() =
            "superuser session from 172.18.0.5 (no application name)\ndatabase litellm is owned by sesh\n"
                .into();
        let c = isolation_check(&bad);
        assert_eq!(c.status, Status::Fail, "specs/007 FR-010");
        assert_eq!(
            c.detail,
            "superuser session from 172.18.0.5 (no application name); database litellm is owned by sesh"
        );
        assert_eq!(c.fix.as_deref(), Some("run `sesh up`"));
        // It is part of every doctor run.
        let (cfg, s) = setup(false);
        let checks = run(
            &cfg,
            &s,
            &[],
            &bad,
            &good_router(true),
            &FakeTailscale(None),
        );
        assert_eq!(status(&checks, "db-isolation"), Status::Fail);
    }

    #[test]
    fn db_isolation_asks_about_every_service_database() {
        let q = isolation_sql();
        for (role, _) in crate::up::SERVICE_DATABASES {
            assert!(q.contains(&format!("('{role}')")), "{role}");
        }
        for needle in [
            "rolsuper",
            "client_addr IS NOT NULL",
            "rolcreatedb",
            "rolcreaterole",
            "datdba",
        ] {
            assert!(q.contains(needle), "{needle}");
        }
    }

    #[test]
    fn doctor_expects_each_service_redis() {
        for r in ["redis-openwebui", "redis-infisical", "redis-paperless"] {
            assert!(SERVICES.contains(&r), "{r}");
        }
        assert!(!SERVICES.contains(&"redis"));
    }

    fn runtime_stack(toolsets: &str, config: &str) -> FakeCompose {
        let f = FakeCompose::healthy(&[("hermes-house", None)]);
        *f.exec_output.borrow_mut() = vec![
            ("hermes-house sh -c curl".into(), toolsets.into()),
            (
                "hermes-house cat /opt/data/config.yaml".into(),
                config.into(),
            ),
        ];
        f
    }

    #[test]
    fn runtime_check_wants_no_builtin_toolset_and_the_written_config() {
        let off =
            r#"{"data":[{"name":"terminal","enabled":false},{"name":"web","enabled":false}]}"#;
        let on = r#"{"data":[{"name":"terminal","enabled":true},{"name":"web","enabled":false}]}"#;
        let good = crate::hermes::config_text("house");
        let r = runtime_check(&["house".into()], &runtime_stack(off, &good));
        assert_eq!(r.status, Status::Pass, "{}", r.detail);
        let f = runtime_stack(on, &good);
        let r = runtime_check(&["house".into()], &f);
        assert_eq!(r.status, Status::Fail);
        assert!(
            r.detail
                .contains("house has built-in toolsets enabled: terminal"),
            "{}",
            r.detail
        );
        // The key is expanded inside the container, never on the host's command line.
        assert!(
            f.calls
                .borrow()
                .iter()
                .any(|c| c.contains("$API_SERVER_KEY"))
        );
        let r = runtime_check(&["house".into()], &runtime_stack(off, "{}"));
        assert!(
            r.detail
                .contains("house's config.yaml isn't the one sesh writes"),
            "{}",
            r.detail
        );
        let r = runtime_check(&["gone".into()], &runtime_stack(off, &good));
        assert!(r.detail.contains("gone isn't running"), "{}", r.detail);
    }

    #[test]
    fn a_browser_that_keeps_restarting_points_at_user_namespaces() {
        let mut f = FakeCompose::healthy(&[("browser-house", None)]);
        assert_eq!(browser_check(&["house".into()], &f).status, Status::Pass);
        f.services.get_mut()[0].state = "restarting".into();
        let c = browser_check(&["house".into()], &f);
        assert_eq!(c.status, Status::Fail);
        assert!(
            c.detail.contains("browser-house is restarting"),
            "{}",
            c.detail
        );
        assert!(
            c.fix
                .unwrap()
                .contains("apparmor_restrict_unprivileged_userns")
        );
        assert!(
            browser_check(&["gone".into()], &f)
                .detail
                .contains("isn't running")
        );
    }
}
