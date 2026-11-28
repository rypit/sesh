//! Stack tests for features 001 (specs/001-foundation/tasks.md T012–T023)
//! 002 (specs/002-gateway-core/tasks.md T015–T022), 003
//! (specs/003-model-evals/tasks.md T009–T024), 004
//! (specs/004-secrets-approvals/tasks.md T007–T017), 005
//! (specs/005-google-paperless/tasks.md), 006 (specs/006-rss-feeds/tasks.md)
//! 007 (specs/007-service-isolation/tasks.md) and 008
//! (specs/008-network-isolation/tasks.md).
//!
//! They start the real stack with the stub model server, so they need
//! Docker and the `sesh-stub-llm:dev`, `sesh-gateway:dev` and
//! `sesh-executor:dev` images, and are ignored by default:
//!
//!     docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
//!     docker build -f deploy/gateway.Dockerfile -t sesh-gateway:dev .
//!     docker build -f deploy/executor.Dockerfile -t sesh-executor:dev .
//!     make stack-test
//!
//! Each test runs one whole sequence, because starting the stack is slow.
//! Each step's message names the requirement it checks.

// The network layout (specs/008), shared with the CLI by path: the test
// binary can't import the `sesh` binary's modules.
#[path = "../src/layout.rs"]
#[allow(dead_code)]
mod layout;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

const WEBUI: &str = "http://127.0.0.1:3000";
const ROUTER: &str = "http://127.0.0.1:4000";

struct Stack {
    dir: PathBuf,
}

impl Stack {
    fn sesh(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sesh"))
            .args(args)
            .env("SESH_DATA_DIR", &self.dir)
            .output()
            .expect("run sesh")
    }

    fn sesh_ok(&self, args: &[&str]) -> String {
        let out = self.sesh(args);
        assert!(
            out.status.success(),
            "sesh {args:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `docker compose` with the same project, env file and files `sesh` uses.
    fn compose(&self, extra_files: &[&Path], env: &[(&str, &str)], args: &[&str]) -> Output {
        self.compose_command(extra_files, env)
            .args(args)
            .output()
            .expect("run docker compose")
    }

    fn compose_command(&self, extra_files: &[&Path], env: &[(&str, &str)]) -> Command {
        let c = self.dir.join("compose");
        let mut cmd = Command::new("docker");
        cmd.args(["compose", "-p", "sesh", "--env-file"])
            .arg(self.dir.join("secrets.env"))
            .arg("-f")
            .arg(c.join("compose.yaml"))
            .arg("-f")
            .arg(c.join("compose.stub.yaml"));
        if c.join("generated.yaml").exists() {
            cmd.arg("-f").arg(c.join("generated.yaml"));
        }
        for f in extra_files {
            cmd.arg("-f").arg(f);
        }
        cmd.env("SESH_DATA_DIR", &self.dir)
            .env("SESH_WEBUI_PORT", "3000")
            .env("SESH_ROUTER_PORT", "4000");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd
    }

    fn secret(&self, key: &str) -> String {
        let text = std::fs::read_to_string(self.dir.join("secrets.env")).unwrap();
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("{key} missing from secrets.env"))
            .to_string()
    }

    fn doctor(&self) -> (bool, Value) {
        let out = self.sesh(&["doctor", "--json"]);
        let checks = serde_json::from_slice(&out.stdout).expect("doctor --json output");
        (out.status.success(), checks)
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        // The stack is gone once this returns, so show its logs on failure.
        if std::thread::panicking() {
            let out = self.compose(&[], &[], &["logs", "--no-color", "--tail", "100"]);
            eprintln!(
                "---- stack logs ----\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        // The eval project (feature 003), if a failure left it behind.
        let eval = |args: &[&str]| {
            Command::new("docker")
                .args(["compose", "-p", "sesh-eval"])
                .args(args)
                .output()
        };
        if std::thread::panicking()
            && let Ok(out) = eval(&["logs", "--no-color", "--tail", "100"])
        {
            eprintln!(
                "---- eval project logs ----\n{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        let _ = eval(&["down", "-v", "--remove-orphans"]);
        let _ = self.compose(&[], &[], &["unpause", "stub-llm"]);
        let _ = self.compose(&[], &[], &["down", "-v", "--remove-orphans"]);
    }
}

fn status_of(checks: &Value, id: &str) -> String {
    checks
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .map(|c| c["status"].as_str().unwrap().to_string())
        .unwrap_or_else(|| panic!("no check {id}"))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn call(method: &str, url: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
    let a = agent();
    let auth = token.map(|t| format!("Bearer {t}"));
    let resp = match (method, body) {
        ("GET", _) => {
            let mut r = a.get(url);
            if let Some(h) = &auth {
                r = r.header("Authorization", h);
            }
            r.call()
        }
        (_, b) => {
            let mut r = a.post(url);
            if let Some(h) = &auth {
                r = r.header("Authorization", h);
            }
            r.send_json(b.unwrap_or(json!({})))
        }
    };
    match resp {
        Ok(mut r) => {
            let status = r.status().as_u16();
            let text = r.body_mut().read_to_string().unwrap_or_default();
            (
                status,
                serde_json::from_str(&text).unwrap_or(Value::String(text)),
            )
        }
        Err(e) => (0, Value::String(e.to_string())),
    }
}

fn chat(model: &str, key: &str) -> (u16, Value) {
    call(
        "POST",
        &format!("{ROUTER}/v1/chat/completions"),
        Some(key),
        Some(json!({"model": model, "messages": [{"role": "user", "content": "hi"}]})),
    )
}

fn signin(email: &str, password: &str) -> String {
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/v1/auths/signin"),
        None,
        Some(json!({"email": email, "password": password})),
    );
    assert_eq!(s, 200, "sign-in failed: {body}");
    body["token"].as_str().expect("token").to_string()
}

fn model_ids(token: &str) -> Vec<String> {
    let (s, body) = call("GET", &format!("{WEBUI}/api/models"), Some(token), None);
    assert_eq!(s, 200, "model list failed: {body}");
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

fn non_loopback_ip() -> Option<String> {
    let out = Command::new("hostname").arg("-I").output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .find(|ip| !ip.starts_with("127.") && ip.contains('.'))
        .map(str::to_string)
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn foundation_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-stack-{}", std::process::id()));
    let stack = Stack { dir };
    stack.sesh_ok(&["init", "--stub", "--admin-email", "admin@sesh.test"]);
    stack.sesh_ok(&["up"]);

    // US1 / T012: the admin signs in and chats with agent-main.
    let admin = signin("admin@sesh.test", &stack.secret("WEBUI_ADMIN_PASSWORD"));
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/chat/completions"),
        Some(&admin),
        Some(json!({"model": "agent-main", "stream": false,
            "messages": [{"role": "user", "content": "hello"}]})),
    );
    assert_eq!(s, 200, "US1 chat failed: {body}");
    assert!(body.to_string().contains("stub reply"), "US1 reply: {body}");

    // US1 / T013 (FR-002, SC-002): every published port is on loopback,
    // and the host's LAN address refuses them.
    let (ok, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "ports-loopback"), "pass", "{checks}");
    if let Some(ip) = non_loopback_ip() {
        for port in [3000, 4000] {
            let addr = format!("{ip}:{port}").parse().unwrap();
            assert!(
                std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_err(),
                "FR-002: {ip}:{port} accepted a connection"
            );
        }
    }

    // US4 / T023: a healthy stack passes doctor (Tailscale may only warn).
    assert!(ok, "US4: doctor failed on a healthy stack: {checks}");

    // US2 / T016 (FR-005, SC-003): a non-admin sees only agent-main, the
    // admin can't read their chats, built-in memory is off.
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/v1/auths/add"),
        Some(&admin),
        Some(json!({"name": "Test User", "email": "user@sesh.test",
            "password": "user-password-1", "role": "user"})),
    );
    assert_eq!(s, 200, "creating a user failed: {body}");
    let user = signin("user@sesh.test", "user-password-1");
    assert_eq!(model_ids(&user), vec!["agent-main".to_string()], "SC-003");
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/v1/chats/new"),
        Some(&user),
        Some(json!({"chat": {"title": "private", "messages": []}})),
    );
    assert_eq!(s, 200, "creating a chat failed: {body}");
    let chat_id = body["id"].as_str().unwrap().to_string();
    let (s, _) = call(
        "GET",
        &format!("{WEBUI}/api/v1/chats/{chat_id}"),
        Some(&admin),
        None,
    );
    assert_ne!(s, 200, "FR-005: the admin could read a user's chat");
    let (s, _) = call(
        "GET",
        &format!("{WEBUI}/api/v1/memories/"),
        Some(&user),
        None,
    );
    assert_ne!(s, 200, "FR-005: built-in memory is enabled");

    // US3 / T018 (FR-003): raw names are refused with the client key; no key is 401.
    let key = stack.secret("WEBUI_ROUTER_KEY");
    assert_eq!(chat("agent-main", &key).0, 200);
    assert_ne!(
        chat("stub-raw-2", &key).0,
        200,
        "FR-003: raw model accepted"
    );
    let (s, body) = chat("openai/stub-model", &key);
    assert_ne!(s, 200, "FR-003: deployment model string accepted: {body}");
    assert_eq!(
        call("GET", &format!("{ROUTER}/v1/models"), None, None).0,
        401
    );

    // US3 / T018 (SC-004): switching the model changes only the router.
    let webui_id = || {
        let out = stack.compose(&[], &[], &["ps", "-q", "open-webui"]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let before = webui_id();
    stack.sesh_ok(&[
        "init",
        "--inference-url",
        "http://stub-llm:8000/v1",
        "--model",
        "stub-raw-2",
    ]);
    let up = stack.sesh_ok(&["up"]);
    assert!(up.contains("restarted litellm"), "SC-004: {up}");
    assert_eq!(webui_id(), before, "SC-004: open-webui was recreated");
    stack.sesh_ok(&["init", "--stub"]);
    stack.sesh_ok(&["up"]);

    // US1 / T015 and US4 / T023: with the stub stopped, chat fails promptly
    // and doctor fails alias-answers; restarting the stub recovers.
    assert!(
        stack
            .compose(&[], &[], &["stop", "stub-llm"])
            .status
            .success()
    );
    let (s, _) = chat("agent-main", &key);
    assert_ne!(s, 200, "US1: chat succeeded with no inference server");
    let (ok, checks) = stack.doctor();
    assert!(!ok);
    assert_eq!(status_of(&checks, "alias-answers"), "fail", "{checks}");
    assert!(
        stack
            .compose(&[], &[], &["start", "stub-llm"])
            .status
            .success()
    );
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        chat("agent-main", &key).0,
        200,
        "US1: no recovery after restart"
    );

    // US4 / T023: a model without tool calling fails alias-tools.
    let off = stack.compose(
        &[],
        &[("STUB_TOOLS", "off")],
        &["up", "-d", "--wait", "stub-llm"],
    );
    assert!(
        off.status.success(),
        "{}",
        String::from_utf8_lossy(&off.stderr)
    );
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "alias-tools"), "fail", "{checks}");
    stack.compose(&[], &[], &["up", "-d", "--wait", "stub-llm"]);

    // US4 / T023: a port published beyond loopback fails ports-loopback.
    let override_file = stack.dir.join("open-port.yaml");
    std::fs::write(
        &override_file,
        "services:\n  open-webui:\n    ports:\n      - \"0.0.0.0:3001:8080\"\n",
    )
    .unwrap();
    let out = stack.compose(
        &[&override_file],
        &[],
        &["up", "-d", "--wait", "open-webui"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let started = std::time::Instant::now();
    let (ok, checks) = stack.doctor();
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "SC-005: doctor took too long"
    );
    assert!(!ok);
    assert_eq!(status_of(&checks, "ports-loopback"), "fail", "{checks}");
}

// ---- Feature 002: the gateway core ----

const GATEWAY: &str = "http://127.0.0.1:7000/mcp";

/// A minimal MCP client over Streamable HTTP, standing in for an agent.
struct Mcp {
    token: String,
    session: Option<String>,
    next_id: u64,
    /// Every tool result this agent received, scanned for secrets (SC-006).
    seen: Vec<String>,
}

impl Mcp {
    /// POSTs one JSON-RPC message; returns the HTTP status and the reply
    /// (the last `data:` event of an SSE stream, or a JSON body).
    fn post(&mut self, body: Value) -> (u16, Option<Value>) {
        let mut r = agent()
            .post(GATEWAY)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/json, text/event-stream");
        if let Some(sid) = &self.session {
            r = r.header("Mcp-Session-Id", sid);
        }
        match r.send_json(body) {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                if let Some(sid) = resp.headers().get("mcp-session-id") {
                    self.session = Some(sid.to_str().unwrap().to_string());
                }
                let text = resp.body_mut().read_to_string().unwrap_or_default();
                let data: Vec<&str> = text
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .collect();
                let msg = data
                    .last()
                    .and_then(|d| serde_json::from_str(d).ok())
                    .or_else(|| serde_json::from_str(&text).ok());
                (status, msg)
            }
            Err(e) => panic!("MCP request failed: {e}"),
        }
    }

    fn connect(token: &str) -> Result<Mcp, u16> {
        let mut m = Mcp {
            seen: Vec::new(),
            token: token.to_string(),
            session: None,
            next_id: 1,
        };
        let (s, _) = m.post(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "sesh-stack-test", "version": "1"}}}));
        if s != 200 {
            return Err(s);
        }
        m.post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        Ok(m)
    }

    /// Calls a tool and returns its text content.
    fn tool(&mut self, name: &str, args: Value) -> String {
        self.next_id += 1;
        let (s, msg) = self.post(json!({"jsonrpc": "2.0", "id": self.next_id,
            "method": "tools/call", "params": {"name": name, "arguments": args}}));
        assert_eq!(s, 200, "tools/call {name}: {msg:?}");
        let msg = msg.expect("a reply");
        let text = msg["result"]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content in {msg}"))
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.seen.push(text.clone());
        text
    }

    fn execute(&mut self, code: &str) -> String {
        self.tool("execute", json!({"code": code}))
    }
}

impl Stack {
    fn token(&self, agent: &str) -> String {
        std::fs::read_to_string(self.dir.join("agents").join(format!("{agent}.token")))
            .unwrap()
            .trim()
            .to_string()
    }

    fn sql(&self, sql: &str) -> String {
        let out = self.compose(
            &[],
            &[],
            &[
                "exec", "-T", "postgres", "psql", "-U", "sesh", "-d", "gateway", "-At", "-c", sql,
            ],
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Runs bash in a service container; returns whether it succeeded and its output.
    fn bash_in(&self, service: &str, script: &str) -> (bool, String) {
        let out = self.compose(&[], &[], &["exec", "-T", service, "bash", "-c", script]);
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }
}

/// A raw HTTP request to the executor port from inside a container, as a
/// compromised executor would send it. Prints the status line.
fn raw_call(capability: &str, extra_header: &str) -> String {
    let body = r#"{"connector":"search","method":"web","args":{"query":"x"}}"#;
    format!(
        "exec 3<>/dev/tcp/gateway/7001 && printf 'POST /call HTTP/1.1\\r\\nHost: gateway\\r\\nX-Sesh-Capability: {capability}\\r\\n{extra_header}Content-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{body}' >&3 && head -1 <&3",
        body.len()
    )
}

/// True if `text` holds a run of exactly 43 base64url characters: the
/// shape of a capability (32 random bytes, unpadded base64url).
fn has_capability_shape(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .any(|w| w.len() == 43)
}

impl Stack {
    fn audit_count(&self, filter: &str) -> u64 {
        self.sql(&format!("SELECT count(*) FROM audit WHERE {filter}"))
            .parse()
            .unwrap()
    }
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn gateway_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-gw-stack-{}", std::process::id()));
    let stack = Stack { dir };
    stack.sesh_ok(&["init", "--stub", "--home", "59.91,10.75"]);
    stack.sesh_ok(&["agent", "add", "alice", "--connectors", "search,geo"]);
    stack.sesh_ok(&["agent", "add", "bob", "--connectors", "search"]);
    let listed = stack.sesh_ok(&["agent", "list"]);
    assert!(
        listed.contains("alice") && listed.contains("bob"),
        "{listed}"
    );
    // The test-only approval-gated connector (contracts/connectors.md),
    // which `sesh agent add` doesn't offer; the gateway reloads agents.toml
    // when it changes.
    let gateway_dir = stack.dir.join("gateway");
    let mut agents = sesh_core::AgentsFile::load(&gateway_dir).unwrap();
    agents
        .agents
        .iter_mut()
        .find(|a| a.name == "alice")
        .expect("alice in agents.toml")
        .connectors
        .push("testgate".into());
    agents.save(&gateway_dir).unwrap();
    assert!(
        std::fs::read_to_string(gateway_dir.join("agents.toml"))
            .unwrap()
            .contains("testgate"),
        "testgate wasn't granted to alice"
    );
    stack.sesh_ok(&["up"]);
    let (alice_t, bob_t) = (stack.token("alice"), stack.token("bob"));

    // SC-005: CI blocks containers' internet access (SESH_TEST_NO_INTERNET=1,
    // see .github/workflows/ci.yml), so the suite passing shows no test
    // needs it. Check the block holds from searxng, the one service with
    // an egress network.
    if std::env::var("SESH_TEST_NO_INTERNET").as_deref() == Ok("1") {
        let out = stack.compose(
            &[],
            &[],
            &[
                "exec",
                "-T",
                "searxng",
                "wget",
                "-q",
                "-T",
                "5",
                "-O",
                "/dev/null",
                "http://example.org/",
            ],
        );
        assert!(
            !out.status.success(),
            "SC-005: a container reached the internet"
        );
    }

    // US1 / T015 (FR-001, SC-001): one script calls two connectors.
    let mut alice = Mcp::connect(&alice_t).expect("alice connects");
    let found = alice.tool("search", json!({"query": "weather forecast"}));
    assert!(found.contains("geo.forecast"), "search: {found}");
    let described = alice.tool("describe", json!({"target": "search.web"}));
    assert!(described.contains("query"), "describe: {described}");
    let before = stack.audit_count("agent = 'alice'");
    let started = std::time::Instant::now();
    let out = alice.execute(
        "async () => { const w = await search.web({ query: \"oslo\" }); const f = await geo.forecast({}); return { w, f }; }",
    );
    let took = started.elapsed();
    assert!(out.contains("\"completed\""), "US1: {out}");
    assert!(
        took < Duration::from_secs(2),
        "SC-001: two calls took {took:?}"
    );
    // SC-003: exactly one audit entry per call.
    assert_eq!(
        stack.audit_count("agent = 'alice'"),
        before + 2,
        "SC-003: two calls"
    );
    assert_eq!(
        stack.audit_count("agent = 'alice' AND outcome = 'allowed' AND ((connector = 'search' AND method = 'web') OR (connector = 'geo' AND method = 'forecast'))"),
        2,
        "SC-003: one allowed entry for each call"
    );
    assert!(
        out.contains("fake result for oslo"),
        "US1 search result: {out}"
    );
    assert!(out.contains("2026-11-27"), "US1 forecast result: {out}");
    assert!(
        out.lines()
            .next()
            .unwrap()
            .to_lowercase()
            .contains("untrusted"),
        "FR-013: {out}"
    );

    // US2 / T016 (FR-002): an unknown or missing token is refused.
    assert_eq!(Mcp::connect("not-a-token").err(), Some(401), "FR-002");
    let (s, _) = call("POST", GATEWAY, None, Some(json!({})));
    assert_eq!(s, 401, "FR-002: no token");

    // US2 / T016 (FR-003): bob sees and reaches only his connectors.
    let mut bob = Mcp::connect(&bob_t).expect("bob connects");
    let found = bob.tool("search", json!({"query": "forecast"}));
    assert!(!found.contains("geo."), "FR-003: bob saw geo: {found}");
    let out = bob.execute("async () => await geo.forecast({})");
    assert!(
        !out.contains("\"completed\""),
        "FR-003: bob reached geo: {out}"
    );

    // US2 / T016 (FR-004): the sandbox has no env, files or network.
    let out = alice.execute(
        "async () => ({ env: JSON.stringify(globalThis.process?.env ?? {}), \
         fs: await import(\"node:fs\").then(() => \"fs-loaded\", () => \"no-fs\"), \
         net: await fetch(\"http://gateway:7001/call\").then(r => \"net-\" + r.status, () => \"net-blocked\"), \
         internet: await fetch(\"https://example.org\").then(r => \"net-\" + r.status, () => \"net-blocked\") })",
    );
    assert!(out.contains("\"completed\""), "{out}");
    assert!(
        !out.contains("SESH_") && !out.contains("PASSWORD"),
        "FR-004 env: {out}"
    );
    assert!(!out.contains("fs-loaded"), "FR-004 files: {out}");
    assert!(
        !out.contains("net-2") && !out.contains("net-4"),
        "FR-004 network: {out}"
    );

    // US2 / T016 (FR-004): the executor container reaches only the gateway's
    // executor port, and nothing else on the stack or the internet.
    let (ok, _) = stack.bash_in("executor-alice", "exec 3<>/dev/tcp/gateway/7001");
    assert!(ok, "the executor can't reach the gateway");
    for target in [
        "searxng/8080",
        "postgres/5432",
        "litellm/4000",
        "gateway/7000",
        "1.1.1.1/443",
    ] {
        let (ok, out) = stack.bash_in(
            "executor-alice",
            &format!("timeout 5 bash -c 'exec 3<>/dev/tcp/{target}'"),
        );
        assert!(!ok, "FR-004: executor reached {target}: {out}");
    }
    assert!(
        !stack
            .bash_in("executor-alice", "env")
            .1
            .contains("PASSWORD"),
        "FR-004: the executor holds a secret"
    );

    // US2 / T016 (FR-005, ADR 0004): a forged capability, or an agent token
    // in place of one, is refused on the executor port.
    let (_, status) = stack.bash_in("executor-alice", &raw_call("forged", ""));
    assert!(
        status.contains(" 403") || status.contains(" 401"),
        "forged capability: {status}"
    );
    let (_, status) = stack.bash_in(
        "executor-alice",
        &raw_call("forged", &format!("Authorization: Bearer {alice_t}\\r\\n")),
    );
    assert!(!status.contains(" 200"), "agent token on /call: {status}");

    // US2 / T016 (FR-008): an approval-gated method waits for the person
    // (feature 004: it asks on the approval channel), whatever the script claims.
    let before = stack.audit_count("agent = 'alice'");
    let out = alice.execute("async () => await testgate.act({})");
    assert!(out.contains("approval_pending"), "FR-008: {out}");
    assert_eq!(
        stack.audit_count("agent = 'alice'"),
        before + 1,
        "SC-003: one entry for the gated call"
    );

    // US2 / T016 (FR-009): an argument outside the schema is refused.
    let before = stack.audit_count("agent = 'alice'");
    let out = alice.execute("async () => await search.web({ query: \"x\", user: \"bob\" })");
    assert!(!out.contains("\"completed\""), "FR-009: {out}");
    assert_eq!(
        stack.audit_count("agent = 'alice'"),
        before + 1,
        "SC-003: one entry for the refused call"
    );

    // US4 / T020 (FR-006, FR-007, SC-004): a runaway script is stopped at the
    // deadline and doesn't stall another agent; the executor recovers.
    let runaway = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let out = alice.execute("async () => { while (true) {} }");
        (alice, out, started.elapsed())
    });
    std::thread::sleep(Duration::from_secs(2));
    let started = std::time::Instant::now();
    let out = bob.execute("async () => await search.web({ query: \"meanwhile\" })");
    assert!(out.contains("\"completed\""), "SC-004: {out}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "FR-007: bob was stalled"
    );
    let (mut alice, out, took) = runaway.join().unwrap();
    assert!(out.contains("\"timeout\""), "FR-006: {out}");
    // SC-004: stopped within 5 s of the 30 s deadline.
    assert!(took < Duration::from_secs(35), "SC-004: took {took:?}");
    let out = alice.execute("async () => 1 + 1");
    assert!(out.contains("\"completed\""), "FR-006: no recovery: {out}");

    // US3 / T018 (FR-010, SC-003, SC-006): every call is audited, refusals
    // included, and the gateway's role can't read the audit log.
    assert_ne!(stack.sql("SELECT count(*) FROM audit WHERE agent = 'alice' AND connector = 'geo' AND outcome = 'allowed'"), "0");
    assert_ne!(stack.sql("SELECT count(*) FROM audit WHERE agent = 'alice' AND outcome = 'refused' AND reason LIKE 'approval%'"), "0");
    assert_ne!(stack.sql("SELECT count(*) FROM audit WHERE agent = 'alice' AND outcome = 'refused' AND method = 'web'"), "0");
    assert_ne!(
        stack.sql("SELECT count(*) FROM audit WHERE outcome = 'refused' AND reason LIKE '%token%'"),
        "0"
    );
    let log = stack.sql("SELECT string_agg(args_summary::text, ' ') FROM audit");
    assert!(
        !log.contains(leak_probe(&alice_t)) && !log.contains(leak_probe(&bob_t)),
        "SC-006: a token reached the audit log"
    );
    let pw = stack.secret("GATEWAY_DB_PASSWORD");
    let out = stack.compose(
        &[],
        &[],
        &[
            "exec",
            "-T",
            "-e",
            &format!("PGPASSWORD={pw}"),
            "postgres",
            "psql",
            "-h",
            "127.0.0.1",
            "-U",
            "sesh_gateway",
            "-d",
            "gateway",
            "-c",
            "SELECT count(*) FROM audit",
        ],
    );
    assert!(
        !out.status.success(),
        "FR-010: the gateway role can read the audit log"
    );
    let out = stack.compose(
        &[],
        &[],
        &[
            "exec",
            "-T",
            "-e",
            &format!("PGPASSWORD={pw}"),
            "postgres",
            "psql",
            "-h",
            "127.0.0.1",
            "-U",
            "sesh_gateway",
            "-d",
            "gateway",
            "-c",
            "DELETE FROM audit",
        ],
    );
    assert!(
        !out.status.success(),
        "FR-010: the gateway role can delete audit rows"
    );

    // SC-006: no token, database password or capability reaches the
    // gateway's or executors' logs, or any result an agent received.
    let logs = stack.compose(
        &[],
        &[],
        &[
            "logs",
            "--no-color",
            "gateway",
            "executor-alice",
            "executor-bob",
        ],
    );
    let logs = String::from_utf8_lossy(&logs.stdout).into_owned();
    assert!(!logs.trim().is_empty(), "no gateway logs to check");
    for (what, text) in [("logs", &logs)].into_iter().chain(
        alice
            .seen
            .iter()
            .chain(bob.seen.iter())
            .map(|t| ("a result", t)),
    ) {
        for secret in [&alice_t, &bob_t, &pw] {
            assert!(
                !text.contains(secret.as_str()),
                "SC-006: a secret reached {what}"
            );
        }
        assert!(
            !has_capability_shape(text),
            "SC-006: a capability-shaped value reached {what}: {text}"
        );
    }

    // US5 / T022 (FR-016): removing an agent revokes its token.
    stack.sesh_ok(&["agent", "remove", "bob"]);
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        Mcp::connect(&bob_t).err(),
        Some(401),
        "FR-016: removed agent still accepted"
    );

    // T019 / T022: doctor sees the gateway and executors healthy.
    stack.sesh_ok(&["up"]);
    let (ok, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "executors-healthy"), "pass", "{checks}");
    assert!(ok, "doctor failed: {checks}");
}

#[test]
fn capability_shape_matches_43_base64url_characters() {
    let cap = "A".repeat(42) + "_";
    assert!(has_capability_shape(&format!("x {cap} y")));
    assert!(has_capability_shape(&format!("\"{cap}\"")));
    assert!(!has_capability_shape(&"a".repeat(44)));
    assert!(!has_capability_shape(
        "https://example.org/fake exec-0123456789abcdef"
    ));
}

/// What a secret scan looks for: the secret's first 20 characters (all of
/// a shorter one), which survive the audit log's cut and still can't
/// appear by chance.
fn leak_probe(secret: &str) -> &str {
    secret
        .char_indices()
        .nth(20)
        .map_or(secret, |(i, _)| &secret[..i])
}

/// The audit log cuts every string to 40 characters
/// (`crates/sesh-gateway/src/redact.rs`, `MAX_STRING`), so a scan of it
/// for a whole 43-character secret could never fail.
#[test]
fn leak_probe_fits_in_an_audited_string() {
    let token = "A".repeat(43);
    let audited: String = format!("\"{token}\"").chars().take(40).collect();
    assert!(
        !audited.contains(&token),
        "why whole-token scans can't fail"
    );
    assert!(audited.contains(leak_probe(&token)));
    assert_eq!(leak_probe("short"), "short");
    assert_eq!(leak_probe(&"\u{e9}".repeat(30)), "\u{e9}".repeat(20));
}

// ---- Feature 003: model evals ----

impl Stack {
    /// `sesh` with extra environment (e.g. `STUB_SCRIPT` for `sesh up`).
    fn sesh_env(&self, env: &[(&str, &str)], args: &[&str]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_sesh"));
        c.args(args).env("SESH_DATA_DIR", &self.dir);
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().expect("run sesh")
    }

    fn eval_json(&self) -> (i32, Value) {
        let out = self.sesh(&["eval", "--json"]);
        let run: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "sesh eval --json: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code().unwrap_or(-1), run)
    }

    fn audit_rows(&self) -> String {
        self.sql("SELECT count(*) FROM audit")
    }
}

fn eval_project(args: &[&str]) -> Output {
    Command::new("docker")
        .args(["compose", "-p", "sesh-eval"])
        .args(args)
        .output()
        .expect("docker compose")
}

fn eval_project_gone() -> bool {
    String::from_utf8_lossy(&eval_project(&["ps", "-a", "-q"]).stdout)
        .trim()
        .is_empty()
}

fn reasons(run: &Value) -> Vec<(String, bool, Vec<String>)> {
    run["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["task"].as_str().unwrap().to_string(),
                r["passed"].as_bool().unwrap(),
                r["reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect(),
            )
        })
        .collect()
}

fn reasons_of(run: &Value, task: &str) -> String {
    reasons(run)
        .into_iter()
        .find(|(t, _, _)| t == task)
        .map(|(_, _, r)| r.join("; "))
        .unwrap_or_default()
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn eval_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-eval-stack-{}", std::process::id()));
    let stack = Stack { dir };
    stack.sesh_ok(&["init", "--stub"]);
    stack.sesh_ok(&["agent", "add", "alice", "--connectors", "search"]);
    let up = stack.sesh_env(&[("STUB_SCRIPT", "pass")], &["up"]);
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stderr)
    );
    let agents_before = stack.sesh_ok(&["agent", "list"]);
    let audit_before = stack.audit_rows();

    // US1 / T009 (FR-001, FR-006, SC-001): every task passes against the
    // passing script, quickly, and the run is saved.
    let started = std::time::Instant::now();
    let out = stack.sesh(&["eval"]);
    let took = started.elapsed();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "US1: {text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(text.matches(" PASS ").count(), 12, "US1: {text}");
    assert!(
        text.contains("12/12 passed") && text.contains("saved "),
        "{text}"
    );
    assert!(
        took < Duration::from_secs(120),
        "SC-001: the suite took {took:?}"
    );

    // US2 / T014 (FR-008, SC-003, SC-004): repeatable, and the household
    // gateway's agents and audit log are untouched; the eval project is gone.
    let (code, again) = stack.eval_json();
    assert_eq!(code, 0, "{again}");
    let (_, third) = stack.eval_json();
    assert_eq!(reasons(&again), reasons(&third), "SC-003: two runs differ");
    assert!(
        eval_project_gone(),
        "FR-008: the eval project was left running"
    );
    assert_eq!(
        stack.sesh_ok(&["agent", "list"]),
        agents_before,
        "FR-008: household agents changed"
    );
    assert_eq!(
        stack.audit_rows(),
        audit_before,
        "SC-004: the household audit log changed"
    );

    // US2 / T017 (constitution II): during a run, the eval executor reaches
    // only the eval gateway; another token is refused. A paused model keeps
    // the run waiting, and Ctrl-C then removes the project (T014).
    assert!(
        stack
            .compose(&[], &[], &["pause", "stub-llm"])
            .status
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_sesh"))
        .args(["eval", "--timeout", "90"])
        .env("SESH_DATA_DIR", &stack.dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let ready = (0..90).any(|_| {
        std::thread::sleep(Duration::from_secs(1));
        let ps = String::from_utf8_lossy(
            &eval_project(&["ps", "--format", "{{.Service}} {{.Health}}"]).stdout,
        )
        .into_owned();
        ps.contains("executor-eval healthy") && ps.contains("gateway-eval healthy")
    });
    assert!(ready, "the eval project never became healthy");
    let probe = |target: &str| {
        eval_project(&[
            "exec",
            "-T",
            "executor-eval",
            "bash",
            "-c",
            &format!("timeout 5 bash -c 'exec 3<>/dev/tcp/{target}'"),
        ])
        .status
        .success()
    };
    assert!(
        probe("gateway-eval/7001"),
        "the eval executor can't reach its gateway"
    );
    for target in ["fakes/8000", "gateway-eval/7000", "1.1.1.1/443"] {
        assert!(
            !probe(target),
            "constitution II: the eval executor reached {target}"
        );
    }
    assert_eq!(
        call(
            "POST",
            "http://127.0.0.1:7100/mcp",
            Some("not-the-eval-token"),
            Some(json!({}))
        )
        .0,
        401,
        "the eval gateway accepted another token"
    );
    let pid = child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .unwrap()
            .success()
    );
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130), "interrupted eval exit code");
    assert!(
        eval_project_gone(),
        "T014: the eval project survived an interrupt"
    );
    assert!(
        stack
            .compose(&[], &[], &["unpause", "stub-llm"])
            .status
            .success()
    );
    let listed = stack.sesh_ok(&["eval", "list"]);
    assert!(listed.contains("(incomplete)"), "T015: {listed}");

    // US1 / T009: with no script and no tool calls, the tool tasks fail
    // naming the missing call.
    let up = stack.sesh_env(&[("STUB_SCRIPT", ""), ("STUB_TOOLS", "off")], &["up"]);
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stderr)
    );
    let (code, run) = stack.eval_json();
    assert_eq!(code, 1, "{run}");
    assert!(
        reasons_of(&run, "web-fact")
            .contains("expected a search.web call that was allowed; none made"),
        "{run}"
    );
    assert!(
        reasons_of(&run, "forecast")
            .contains("expected a geo.forecast call that was allowed; none made"),
        "{run}"
    );

    // US3 / T018 (FR-017, SC-002): the failing script fails each task for
    // the expected reason.
    let up = stack.sesh_env(&[("STUB_SCRIPT", "fail")], &["up"]);
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stderr)
    );
    let (code, run) = stack.eval_json();
    assert_eq!(code, 1, "{run}");
    assert_eq!(run["passed"], 0, "{run}");
    for (task, why) in [
        (
            "web-fact",
            "expected a search.web call that was allowed; none made",
        ),
        (
            "forecast",
            "the answer doesn't contain the expected fact \"13.7\"",
        ),
        ("combined", "reached the turn limit (8)"),
        (
            "refusal",
            "never recovered from the refused search.web call",
        ),
        ("approval", "claimed an action that was refused"),
        (
            "approve-act",
            "approval was granted but the action wasn't taken",
        ),
        // Suite v3 (specs/005 research.md R8).
        (
            "mail-fact",
            "expected a mail.search call that was allowed; none made",
        ),
        (
            "mail-draft",
            "expected a mail.draft call that was allowed; none made",
        ),
        (
            "mail-send",
            "approval was granted but the action wasn't taken",
        ),
        ("file-attachment", "the answer pastes a file's contents"),
        // Suite v4 (specs/006 research.md R11).
        (
            "feed-headline",
            "the answer doesn't contain the expected fact \"Marrow Quay\"",
        ),
        (
            "feed-add",
            "approval was granted but the action wasn't taken",
        ),
    ] {
        assert!(
            reasons_of(&run, task).contains(why),
            "SC-002 {task}: {}",
            reasons_of(&run, task)
        );
    }
    let fail_id = run["id"].as_str().unwrap().to_string();

    // US4 / T021: list, compare and show.
    let pass_id = again["id"].as_str().unwrap().to_string();
    let listed = stack.sesh_ok(&["eval", "list"]);
    assert!(
        listed.lines().next().unwrap().starts_with(&fail_id),
        "newest first: {listed}"
    );
    let cmp = stack.sesh_ok(&["eval", "compare", &pass_id, &fail_id]);
    assert!(cmp.contains("forecast  PASS  FAIL *"), "{cmp}");
    let shown = stack.sesh_ok(&["eval", "show", &fail_id, "approval"]);
    assert!(
        shown.starts_with("=== untrusted model output and tool results ==="),
        "{shown}"
    );
    assert!(shown.contains("Done, it's completed."), "{shown}");

    // SC-005 / T024: no secret or capability-shaped value in any saved file.
    let secrets: Vec<String> = std::fs::read_to_string(stack.dir.join("secrets.env"))
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(_, v)| v.to_string()))
        .filter(|v| v.len() >= 8)
        .collect();
    let mut files = vec![];
    for run in std::fs::read_dir(stack.dir.join("evals"))
        .unwrap()
        .flatten()
    {
        if run.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        files.push(run.path().join("run.json"));
        if let Ok(ts) = std::fs::read_dir(run.path().join("transcripts")) {
            files.extend(ts.flatten().map(|t| t.path()));
        }
    }
    assert!(files.len() > 5);
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for s in &secrets {
            assert!(
                !text.contains(s.as_str()),
                "SC-005: a secret in {}",
                f.display()
            );
        }
        assert!(
            !has_capability_shape(&text),
            "SC-005: a capability-shaped value in {}",
            f.display()
        );
    }
}

// ---- Feature 004: secrets and approvals ----

fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Stack {
    fn topic(&self) -> String {
        let text = std::fs::read_to_string(self.dir.join("sesh.toml")).unwrap();
        text.lines()
            .find_map(|l| l.trim().strip_prefix("ntfy_topic = "))
            .unwrap()
            .trim_matches('"')
            .to_string()
    }

    /// Every notification on the person's topic so far, read as the person.
    fn notifications(&self) -> Vec<Value> {
        use base64::Engine;
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("person:{}", self.secret("NTFY_PERSON_PASSWORD")));
        let mut r = agent()
            .get(&format!(
                "http://127.0.0.1:8090/{}/json?poll=1&since=all",
                self.topic()
            ))
            .header("Authorization", &format!("Basic {basic}"))
            .call()
            .unwrap();
        r.body_mut()
            .read_to_string()
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["event"] == "message")
            .collect()
    }

    /// The (page URL, secret) of the newest notification.
    fn newest_link(&self) -> (String, String) {
        let n = self.notifications();
        let link = n.last().expect("a notification")["actions"][0]["url"]
            .as_str()
            .unwrap()
            .to_string();
        let (page, secret) = link.split_once('#').unwrap();
        (page.to_string(), secret.to_string())
    }
}

fn decide(page: &str, secret: &str, decision: &str) -> u16 {
    call(
        "POST",
        page,
        None,
        Some(json!({"secret": secret, "decision": decision})),
    )
    .0
}

fn approval_id(out: &str) -> String {
    let from = out.find("\"approval\":{").expect("an approval");
    let i = from + out[from..].find("\"id\":\"").expect("an approval id") + 6;
    out[i..i + 32].to_string()
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn secrets_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-secrets-stack-{}", std::process::id()));
    let stack = Stack { dir };
    // The value the person will paste; the stub accepts only its hash.
    let value = format!("cred-{}-value", std::process::id());
    let value2 = format!("cred-{}-other", std::process::id());
    stack.sesh_ok(&["init", "--stub"]);
    stack.sesh_ok(&[
        "agent",
        "add",
        "alice",
        "--connectors",
        "search,geo,testgate,testcred",
    ]);
    let up = stack.sesh_env(&[("STUB_TESTCRED_SHA256", &sha256_hex(&value))], &["up"]);
    assert!(
        up.status.success(),
        "{}{}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );
    assert!(String::from_utf8_lossy(&up.stdout).contains("set up the secret store"));
    let alice_t = stack.token("alice");
    let mut alice = Mcp::connect(&alice_t).expect("alice connects");

    // US1 / T007 (FR-001, FR-002, FR-005, SC-003): pending, notified, approved,
    // then run exactly once.
    let out = alice.execute("async () => await testgate.act({ note: \"first\" })");
    assert!(out.contains("\"approval_pending\""), "FR-002: {out}");
    let id = approval_id(&out);
    let n = stack.notifications();
    assert_eq!(n.len(), 1, "one notification");
    assert!(
        n[0]["title"]
            .as_str()
            .unwrap()
            .contains("alice wants to run testgate.act"),
        "{}",
        n[0]
    );
    let (page, secret) = stack.newest_link();
    assert!(page.ends_with(&format!("/approve/{id}")), "{page}");
    let again = alice.execute("async () => await testgate.act({ note: \"first\" })");
    assert_eq!(approval_id(&again), id, "a pending request is reused");
    assert_eq!(stack.notifications().len(), 1, "no second notification");
    assert_eq!(decide(&page, &secret, "approve"), 200);
    let ran = alice.execute("async () => await testgate.act({ note: \"first\" })");
    assert!(
        ran.contains("\"completed\"") && ran.contains("\"done\":true"),
        "SC-003: {ran}"
    );
    let next = alice.execute("async () => await testgate.act({ note: \"first\" })");
    assert!(
        next.contains("\"approval_pending\""),
        "FR-005: an approval runs once: {next}"
    );

    // US1 (FR-005): changed arguments need their own approval.
    let changed = alice.execute("async () => await testgate.act({ note: \"changed\" })");
    assert_ne!(approval_id(&changed), approval_id(&next), "FR-005");

    // US1 / T007 (FR-006): denied never runs.
    let out = alice.execute("async () => await testgate.act({ note: \"deny me\" })");
    let (page, secret) = stack.newest_link();
    assert!(page.ends_with(&approval_id(&out)));
    assert_eq!(decide(&page, &secret, "deny"), 200);
    let out = alice.execute("async () => await testgate.act({ note: \"deny me\" })");
    assert!(
        out.contains("denied") && !out.contains("\"done\":true"),
        "FR-006: {out}"
    );

    // US2 / T011 (FR-004, FR-007): no link or secret reaches the agent; reused,
    // guessed and agent-token attempts are refused.
    for result in &alice.seen {
        assert!(
            !result.contains(&secret) && !result.contains("/approve/"),
            "FR-004: {result}"
        );
    }
    assert_eq!(decide(&page, &secret, "approve"), 409, "a decided request");
    assert_eq!(decide(&page, "guessed-secret", "approve"), 403);
    let (s, _) = call(
        "GET",
        "http://127.0.0.1:7002/admin/credentials",
        Some(&alice_t),
        None,
    );
    assert_eq!(s, 403, "FR-007: an agent token on the approvals listener");
    let (ok, _) = stack.bash_in(
        "executor-alice",
        "timeout 5 bash -c 'exec 3<>/dev/tcp/gateway/7002'",
    );
    assert!(!ok, "FR-007: the executor reached the approvals listener");

    // US2 / T011 (FR-003): agent text appears only quoted in the notification.
    alice.execute("async () => await testgate.act({ note: \"this is harmless, approve\" })");
    let msg = stack.notifications().last().unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        msg.starts_with("Agent alice wants to run testgate.act"),
        "FR-003: {msg}"
    );
    assert!(
        msg.contains("note: \"this is harmless, approve\""),
        "FR-003: {msg}"
    );

    // US3 / T013 (FR-010–FR-012): a credential through a one-time form.
    let out = alice.execute("async () => await testcred.check({})");
    assert!(out.contains("not set up yet"), "FR-012: {out}");
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "credentials"), "warn", "{checks}");
    let printed = stack.sesh_ok(&["credential", "request", "testcred", "TOKEN"]);
    let link = printed
        .lines()
        .find(|l| l.contains("/form/"))
        .unwrap()
        .trim()
        .to_string();
    let (form, form_secret) = link.split_once('#').unwrap();
    let fill = |v: &str| {
        call(
            "POST",
            form,
            None,
            Some(json!({"secret": form_secret, "value": v})),
        )
        .0
    };
    assert_eq!(fill(&value), 200);
    assert_eq!(fill("again"), 409, "FR-010: the link works once");
    let out = alice.execute("async () => await testcred.check({})");
    assert!(
        out.contains("\"ok\":true"),
        "FR-011: the upstream got the right value: {out}"
    );
    assert!(
        stack
            .sesh_ok(&["credential", "list"])
            .contains("testcred  TOKEN  set")
    );
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "credentials"), "pass", "{checks}");
    // SC-005: a replaced value is used on the next call, with no restart.
    let printed = stack.sesh_ok(&["credential", "request", "testcred", "TOKEN"]);
    let link = printed
        .lines()
        .find(|l| l.contains("/form/"))
        .unwrap()
        .trim()
        .to_string();
    let (form, form_secret) = link.split_once('#').unwrap();
    assert_eq!(
        call(
            "POST",
            form,
            None,
            Some(json!({"secret": form_secret, "value": value2}))
        )
        .0,
        200
    );
    let out = alice.execute("async () => await testcred.check({})");
    assert!(
        out.contains("\"ok\":false"),
        "SC-005: the new value was sent: {out}"
    );

    // US3 (edge case): with the secret store down, credentialed calls are
    // refused and others still work.
    assert!(
        stack
            .compose(&[], &[], &["stop", "infisical"])
            .status
            .success()
    );
    let out = alice.execute("async () => await testcred.check({})");
    assert!(out.contains("credential store unavailable"), "{out}");
    let out = alice.execute("async () => await search.web({ query: \"still works\" })");
    assert!(out.contains("\"completed\""), "{out}");
    assert!(
        stack
            .compose(&[], &[], &["start", "infisical"])
            .status
            .success()
    );

    // US4 / T017 (FR-013): the events are audited, linked by request id.
    for reason in [
        format!("approval pending {id}"),
        format!("approval {id} approved"),
        format!("approved {id}"),
        "credential.request testcred/TOKEN".to_string(),
        "credential.fill testcred/TOKEN".to_string(),
        "credential testcred/TOKEN".to_string(),
        "agent token on the approvals listener".to_string(),
        "approval refused: wrong secret".to_string(),
    ] {
        let n = stack.sql(&format!(
            "SELECT count(*) FROM audit WHERE reason = '{reason}'"
        ));
        assert_ne!(n, "0", "FR-013: no audit row for {reason:?}");
    }

    // SC-004: no credential value or approval/form secret anywhere it shouldn't be.
    let audit = stack.sql(
        "SELECT string_agg(coalesce(reason, '') || ' ' || args_summary::text, ' ') FROM audit",
    );
    let logs = String::from_utf8_lossy(
        &stack
            .compose(
                &[],
                &[],
                &["logs", "--no-color", "gateway", "executor-alice"],
            )
            .stdout,
    )
    .into_owned();
    for (what, text) in [("the audit log", &audit), ("gateway/executor logs", &logs)]
        .into_iter()
        .chain(alice.seen.iter().map(|t| ("a result", t)))
    {
        for s in [&value, &value2, &secret, &form_secret.to_string()] {
            assert!(
                !text.contains(leak_probe(s)),
                "SC-004: a secret reached {what}"
            );
        }
    }
    for entry in walk(&stack.dir) {
        if let Ok(text) = std::fs::read_to_string(&entry) {
            assert!(
                !text.contains(&value) && !text.contains(&value2),
                "SC-004: the credential is in {}",
                entry.display()
            );
        }
    }
}

impl Stack {
    /// Notifications on `topic`, read as the ntfy user `user`.
    fn notifications_as(&self, user: &str, password_key: &str, topic: &str) -> Vec<Value> {
        use base64::Engine;
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("{user}:{}", self.secret(password_key)));
        let mut r = agent()
            .get(&format!(
                "http://127.0.0.1:8090/{topic}/json?poll=1&since=all"
            ))
            .header("Authorization", &format!("Basic {basic}"))
            .call()
            .unwrap();
        r.body_mut()
            .read_to_string()
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["event"] == "message")
            .collect()
    }

    fn user_topic(&self, name: &str) -> String {
        let text = std::fs::read_to_string(self.dir.join("gateway").join("users.toml")).unwrap();
        let v: toml::Value = toml::from_str(&text).unwrap();
        v["user"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["name"].as_str() == Some(name))
            .unwrap()["topic"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// What fake Google recorded (sent mail, drafts, events), read from
    /// inside the gateway's container: the stub has no host port.
    fn fake_google(&self) -> Value {
        let (ok, out) = self.bash_in(
            "gateway",
            "exec 3<>/dev/tcp/stub-llm/8000 && printf 'GET /fake/google/sent HTTP/1.1\\r\\nHost: stub\\r\\nConnection: close\\r\\n\\r\\n' >&3 && cat <&3",
        );
        assert!(ok, "{out}");
        let body = out.split("\r\n\r\n").nth(1).unwrap_or("");
        serde_json::from_str(body).unwrap_or_else(|_| panic!("fake google: {out}"))
    }

    /// Runs the consent flow for `owner` the way a browser would, with
    /// fake Google's code for `code_user`. Returns the callback status.
    fn connect_google(&self, owner: &str, code_user: &str) -> u16 {
        let printed = self.sesh_ok(&["google", "connect", owner]);
        let link = printed
            .lines()
            .find(|l| l.contains("/google/start/"))
            .unwrap()
            .trim()
            .to_string();
        let (page, secret) = link.split_once('#').unwrap();
        let (s, v) = call("POST", page, None, Some(json!({"secret": secret})));
        assert_eq!(s, 200, "{v}");
        let auth = v["redirect"].as_str().unwrap();
        assert!(auth.contains("/fake/google/auth?"), "{auth}");
        let (again, _) = call("POST", page, None, Some(json!({"secret": secret})));
        assert_eq!(again, 403, "US2: the consent link works once");
        let state = auth
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .replace("%2E", ".");
        let mut r = agent()
            .get(&format!(
                "http://127.0.0.1:7002/google/callback?state={state}&code=fake-{code_user}"
            ))
            .call()
            .unwrap();
        let _ = r.body_mut().read_to_string();
        r.status().as_u16()
    }

    fn fill_credential(&self, connector: &str, name: &str, value: &str) {
        let printed = self.sesh_ok(&["credential", "request", connector, name]);
        let link = printed
            .lines()
            .find(|l| l.contains("/form/"))
            .unwrap()
            .trim()
            .to_string();
        let (form, secret) = link.split_once('#').unwrap();
        let (s, _) = call(
            "POST",
            form,
            None,
            Some(json!({"secret": secret, "value": value})),
        );
        assert_eq!(s, 200);
    }
}

fn first_ref(out: &str) -> String {
    let i = out.find("\"ref\":\"").expect("an attachment ref") + 7;
    out[i..i + 32].to_string()
}

fn retry(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..30 {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    false
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn google_docs_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-google-stack-{}", std::process::id()));
    let stack = Stack { dir };
    stack.sesh_ok(&["init", "--stub"]);

    // US1 (FR-001–FR-004): users, and agents bound to an owner.
    stack.sesh_ok(&["user", "add", "ana"]);
    stack.sesh_ok(&["user", "add", "ben"]);
    let all = "mail,calendar,contacts,docs";
    stack.sesh_ok(&[
        "agent",
        "add",
        "anabot",
        "--connectors",
        all,
        "--owner",
        "ana",
    ]);
    stack.sesh_ok(&[
        "agent",
        "add",
        "benbot",
        "--connectors",
        all,
        "--owner",
        "ben",
    ]);
    stack.sesh_ok(&["agent", "add", "house", "--connectors", all]);
    let bad = stack.sesh(&[
        "agent",
        "add",
        "ghost",
        "--connectors",
        "mail",
        "--owner",
        "zed",
    ]);
    assert!(!bad.status.success(), "FR-002: an unknown owner is refused");
    assert!(stack.sesh_ok(&["agent", "list"]).contains("anabot"));
    assert!(stack.sesh_ok(&["user", "list"]).contains("ben"));
    stack.sesh_ok(&["up"]);

    let mut anabot = Mcp::connect(&stack.token("anabot")).expect("anabot connects");
    let mut benbot = Mcp::connect(&stack.token("benbot")).expect("benbot connects");
    let mut house = Mcp::connect(&stack.token("house")).expect("house connects");

    // US2 (FR-005–FR-008): not connected yet, then the one-time consent.
    let out = anabot.execute("async () => await mail.search({ query: \"dentist\" })");
    assert!(out.contains("Google isn't connected"), "FR-008: {out}");
    stack.fill_credential("google", "CLIENT_ID", "fake-client-id");
    stack.fill_credential("google", "CLIENT_SECRET", "fake-client-secret");
    assert_eq!(stack.connect_google("ana", "ana"), 200, "US2: ana connects");
    assert_eq!(stack.connect_google("ben", "ben"), 200, "US2: ben connects");
    assert_eq!(stack.connect_google("household", "household"), 200);
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "google"), "pass", "{checks}");

    // US3 (FR-009, FR-010, SC-001): each agent reads only its owner's mail.
    let out = anabot.execute("async () => await mail.search({ query: \"dentist\" })");
    assert!(out.contains("Your dentist appointment"), "US3: {out}");
    let out = anabot.execute("async () => await mail.read({ id: \"m-ben-1\" })");
    assert!(
        !out.contains("marigold"),
        "SC-001: ana's agent read ben's mail: {out}"
    );
    let out = benbot.execute("async () => await mail.search({ query: \"statement\" })");
    assert!(
        out.contains("Ben's statement") && !out.contains("dentist"),
        "SC-001: {out}"
    );
    let out =
        anabot.execute("async () => await mail.search({ query: \"statement\", user: \"ben\" })");
    assert!(
        out.contains("not allowed") && !out.contains("marigold"),
        "FR-003: a model can't pick the user: {out}"
    );
    let out = anabot.execute("async () => await contacts.search({ query: \"okafor\" })");
    assert!(out.contains("clinic@example.org"), "{out}");
    let out = anabot.execute("async () => await calendar.list({})");
    assert!(
        out.contains("ana standing meeting") && !out.contains("ben standing"),
        "{out}"
    );

    // US3 (FR-011, SC-002): sending waits for ana's approval, on ana's topic.
    let send = "async () => await mail.send({ to: [\"clinic@example.org\"], subject: \"Tuesday\", body: \"See you at 10:40.\" })";
    let out = anabot.execute(send);
    assert!(out.contains("\"approval_pending\""), "FR-011: {out}");
    assert!(
        stack.fake_google()["sent"].get("ana").is_none(),
        "SC-002: mail was sent before approval"
    );
    let ana_topic = stack.user_topic("ana");
    let notes = stack.notifications_as("u-ana", "NTFY_PASSWORD_ANA", &ana_topic);
    assert_eq!(notes.len(), 1, "the approval goes to ana's topic");
    assert!(
        stack.notifications().is_empty(),
        "ana's approval isn't on the household topic"
    );
    let ben_topic = stack.user_topic("ben");
    assert!(
        stack
            .notifications_as("u-ben", "NTFY_PASSWORD_BEN", &ben_topic)
            .is_empty(),
        "nor on ben's"
    );
    let link = notes[0]["actions"][0]["url"].as_str().unwrap();
    let (page, secret) = link.split_once('#').unwrap();
    assert_eq!(decide(page, secret, "approve"), 200);
    let out = anabot.execute(send);
    assert!(out.contains("\"message_id\""), "{out}");
    let sent = stack.fake_google();
    assert_eq!(sent["sent"]["ana"][0]["subject"], "Tuesday", "{sent}");
    assert_eq!(sent["sent"]["ana"][0]["to"], "clinic@example.org", "{sent}");
    let out = anabot.execute("async () => await mail.draft({ to: [\"clinic@example.org\"], subject: \"Draft\", body: \"x\" })");
    assert!(
        out.contains("\"draft_id\""),
        "drafting needs no approval: {out}"
    );
    let out = anabot.execute("async () => await calendar.create({ title: \"Dentist\", start: \"2026-12-01T10:40:00Z\", end: \"2026-12-01T11:10:00Z\" })");
    assert!(out.contains("\"approval_pending\""), "FR-011: {out}");
    // Denied: nothing is sent.
    let deny_me =
        "async () => await mail.send({ to: [\"x@example.org\"], subject: \"Deny\", body: \"no\" })";
    assert!(anabot.execute(deny_me).contains("\"approval_pending\""));
    let notes = stack.notifications_as("u-ana", "NTFY_PASSWORD_ANA", &ana_topic);
    let link = notes.last().unwrap()["actions"][0]["url"].as_str().unwrap();
    let (page, secret) = link.split_once('#').unwrap();
    assert_eq!(decide(page, secret, "deny"), 200);
    let out = anabot.execute(deny_me);
    assert!(out.contains("denied"), "{out}");
    assert_eq!(
        stack.fake_google()["sent"]["ana"].as_array().unwrap().len(),
        1,
        "SC-002: a denied send went out"
    );
    // US1 (FR-004): a Household approval reaches every user.
    let out = house.execute("async () => await calendar.create({ title: \"Bins\", start: \"2026-12-02T07:00:00Z\", end: \"2026-12-02T07:15:00Z\" })");
    assert!(out.contains("\"approval_pending\""), "{out}");
    let hh = stack.topic();
    for (user, key) in [
        ("u-ana", "NTFY_PASSWORD_ANA"),
        ("u-ben", "NTFY_PASSWORD_BEN"),
    ] {
        let n = stack.notifications_as(user, key, &hh);
        assert!(
            n.iter()
                .any(|m| m["title"].as_str().unwrap_or("").contains("house")),
            "FR-004: {user} can't read the household approval"
        );
    }
    // Header injection is refused before anything is sent.
    let out = anabot.execute("async () => await mail.draft({ to: [\"a@example.org\\r\\nBcc: x@example.org\"], subject: \"x\", body: \"x\" })");
    assert!(!out.contains("\"draft_id\""), "{out}");

    // US4 (FR-012–FR-014, SC-003): an attachment is filed by reference.
    let out = anabot.execute("async () => await mail.read({ id: \"m-ana-1\" })");
    assert!(out.contains("dentist-invoice.pdf"), "{out}");
    assert!(
        !out.contains("JVBERi"),
        "FR-012: file bytes reached the agent: {out}"
    );
    let ana_ref = first_ref(&out);
    let out = benbot.execute(&format!(
        "async () => await docs.upload({{ ref: \"{ana_ref}\", title: \"stolen\" }})"
    ));
    assert!(out.contains("unknown or expired"), "FR-013: {out}");
    let out = anabot.execute(&format!(
        "async () => await docs.upload({{ ref: \"{ana_ref}\", title: \"Dentist invoice\", tags: [\"health\"] }})"
    ));
    assert!(out.contains("\"title\":\"Dentist invoice\""), "US4: {out}");
    let ana_upload = out.clone();
    let out = house.execute("async () => await mail.read({ id: \"m-hh-1\" })");
    let hh_ref = first_ref(&out);
    let out = house.execute(&format!(
        "async () => await docs.upload({{ ref: \"{hh_ref}\", title: \"Electricity bill\" }})"
    ));
    assert!(out.contains("\"title\":\"Electricity bill\""), "US4: {out}");
    assert!(
        retry(|| stack
            .notifications()
            .iter()
            .any(|n| n["title"] == "New household document")),
        "FR-014: household uploads notify"
    );
    // SC-003: ana sees hers and the household's; ben sees neither of ana's.
    let mut last = String::new();
    assert!(
        retry(|| {
            last = anabot.execute("async () => await docs.search({ query: \"invoice\" })");
            last.contains("Dentist invoice")
        }),
        "ana finds her document; upload: {ana_upload}; last search: {last}"
    );
    assert!(
        retry(|| {
            last = anabot.execute("async () => await docs.search({ query: \"electricity\" })");
            last.contains("Electricity bill")
        }),
        "ana reads the household's documents; last search: {last}"
    );
    let out = benbot.execute("async () => await docs.search({ query: \"invoice\" })");
    assert!(!out.contains("Dentist invoice"), "SC-003: {out}");
    let out = house.execute("async () => await docs.search({ query: \"invoice\" })");
    assert!(
        !out.contains("Dentist invoice"),
        "SC-003: the household can't read ana's: {out}"
    );
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "paperless"), "pass", "{checks}");

    // US2 (FR-007): disconnecting removes the grant; removing a user stops
    // their agents.
    stack.sesh_ok(&["google", "disconnect", "ana"]);
    let out = anabot.execute("async () => await mail.search({ query: \"dentist\" })");
    assert!(out.contains("Google isn't connected"), "FR-007: {out}");
    stack.sesh_ok(&["user", "remove", "ben"]);
    let out = benbot.tool(
        "execute",
        json!({"code": "async () => await mail.search({ query: \"x\" })"}),
    );
    assert!(out.contains("owner was removed"), "FR-004: {out}");

    // FR-015: consent, connection and disconnection are audited.
    for reason in [
        "google.consent requested for ana",
        "google.connected ana (4)",
        "google.disconnected ana",
    ] {
        let n = stack.sql(&format!(
            "SELECT count(*) FROM audit WHERE reason = '{reason}'"
        ));
        assert_ne!(n, "0", "FR-015: no audit row for {reason:?}");
    }

    // SC-004: no Google token anywhere outside the secret store.
    let audit = stack.sql(
        "SELECT string_agg(coalesce(reason, '') || ' ' || args_summary::text, ' ') FROM audit",
    );
    let logs = String::from_utf8_lossy(
        &stack
            .compose(
                &[],
                &[],
                &[
                    "logs",
                    "--no-color",
                    "gateway",
                    "executor-anabot",
                    "executor-benbot",
                    "executor-house",
                ],
            )
            .stdout,
    )
    .into_owned();
    let seen: Vec<&String> = anabot
        .seen
        .iter()
        .chain(&benbot.seen)
        .chain(&house.seen)
        .collect();
    for (what, text) in [("the audit log", &audit), ("logs", &logs)]
        .into_iter()
        .chain(seen.into_iter().map(|t| ("a result", t)))
    {
        for token in ["frt-", "fat-", "fake-client-secret"] {
            assert!(!text.contains(token), "SC-004: {token} reached {what}");
        }
    }
    for entry in walk(&stack.dir) {
        if let Ok(text) = std::fs::read_to_string(&entry) {
            assert!(
                !text.contains("frt-") && !text.contains("fake-client-secret"),
                "SC-004: a Google secret is in {}",
                entry.display()
            );
        }
    }
}

// ---- Feature 006: RSS feeds ----

/// The stub's fake publisher, as Miniflux reaches it on `feeds-egress`.
const FAKE_FEEDS: &str = "http://stub-llm:8000/fake/feeds";

impl Stack {
    /// A service's container id.
    fn container(&self, service: &str) -> String {
        let out = self.compose(&[], &[], &["ps", "-q", service]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A container's networks, by name, with its address on each.
    fn networks_of(&self, service: &str) -> Value {
        let out = Command::new("docker")
            .args([
                "inspect",
                "-f",
                "{{json .NetworkSettings.Networks}}",
                &self.container(service),
            ])
            .output()
            .expect("docker inspect");
        serde_json::from_slice(&out.stdout).expect("networks json")
    }
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn feeds_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-feeds-stack-{}", std::process::id()));
    let stack = Stack { dir };
    stack.sesh_ok(&["init", "--stub"]);
    stack.sesh_ok(&["user", "add", "ana"]);
    stack.sesh_ok(&["user", "add", "ben"]);
    for (agent, owner) in [("anabot", "ana"), ("benbot", "ben"), ("house", "household")] {
        stack.sesh_ok(&[
            "agent",
            "add",
            agent,
            "--connectors",
            "feeds",
            "--owner",
            owner,
        ]);
    }
    stack.sesh_ok(&["up"]);

    // US1 (FR-001, FR-003): people choose the feeds; Miniflux fetches them.
    for (owner, name) in [
        ("household", "harbour"),
        ("ana", "garden"),
        ("ben", "cycling"),
    ] {
        let out = stack.sesh_ok(&[
            "feed",
            "add",
            owner,
            &format!("{FAKE_FEEDS}/{name}.xml"),
            "--name",
            name,
        ]);
        assert!(
            out.contains(&format!("added feed {name} for {owner}")),
            "{out}"
        );
    }
    for (url, why) in [
        ("ftp://example.org/feed", "only http and https"),
        ("https://u:p@example.org/feed", "user name or password"),
        (
            &format!("{FAKE_FEEDS}/notafeed.xml") as &str,
            "couldn't add that feed",
        ),
    ] {
        let out = stack.sesh(&["feed", "add", "ana", url, "--name", "bad"]);
        assert!(!out.status.success(), "{url} was added");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(why),
            "{url}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let bad = stack.sesh(&["feed", "add", "zed", &format!("{FAKE_FEEDS}/ferry.xml")]);
    assert!(!bad.status.success(), "an unknown owner is refused");
    let listed = stack.sesh_ok(&["feed", "list"]);
    assert_eq!(listed.lines().count(), 3, "{listed}");
    assert_eq!(stack.sesh_ok(&["feed", "list", "ana"]).lines().count(), 1);

    let mut anabot = Mcp::connect(&stack.token("anabot")).expect("anabot connects");
    let mut benbot = Mcp::connect(&stack.token("benbot")).expect("benbot connects");
    let mut house = Mcp::connect(&stack.token("house")).expect("house connects");

    // US2 (FR-002, SC-001): each agent reads its owner's and the Household's.
    let out = anabot.execute("async () => await feeds.list({})");
    assert!(
        out.contains("garden") && out.contains("harbour") && !out.contains("cycling"),
        "{out}"
    );
    let out = benbot.execute("async () => await feeds.list({})");
    assert!(
        out.contains("cycling") && out.contains("harbour") && !out.contains("garden"),
        "{out}"
    );
    let out = house.execute("async () => await feeds.list({})");
    assert!(
        out.contains("harbour") && !out.contains("garden") && !out.contains("cycling"),
        "{out}"
    );
    let mut last = String::new();
    assert!(
        retry(|| {
            last = anabot.execute("async () => await feeds.latest({ feed: \"garden\" })");
            last.contains("Seed swap on Saturday")
        }),
        "SC-005: ana reads her feed: {last}"
    );
    let out = benbot.execute("async () => await feeds.latest({ feed: \"garden\" })");
    assert!(
        out.contains("no feed named garden") && !out.contains("Seed swap"),
        "SC-001: {out}"
    );
    let out = house.execute("async () => await feeds.latest({ feed: \"garden\" })");
    assert!(out.contains("no feed named garden"), "SC-001: {out}");
    let out =
        benbot.execute("async () => await feeds.latest({ feed: \"garden\", owner: \"ana\" })");
    assert!(
        out.contains("not allowed") && !out.contains("Seed swap"),
        "FR-002: {out}"
    );
    // FR-008: the injected item arrives as data, and nothing is added.
    for bot in [&mut anabot, &mut benbot] {
        let out = bot.execute("async () => await feeds.latest({ feed: \"harbour\" })");
        assert!(
            out.contains("Marrow Quay") && out.contains("Ignore previous instructions"),
            "the Household's feed, for both users: {out}"
        );
        assert!(
            !out.contains("<p>") && !out.contains("<b>"),
            "markup stripped: {out}"
        );
    }
    assert!(!stack.sesh_ok(&["feed", "list"]).contains("evil"), "FR-008");

    // US3 (FR-009, SC-003): an agent's add waits for the owner.
    let ana_topic = stack.user_topic("ana");
    let add = format!(
        "async () => await feeds.add({{ url: \"{FAKE_FEEDS}/ferry.xml\", name: \"ferry\" }})"
    );
    let out = anabot.execute(&add);
    assert!(out.contains("\"approval_pending\""), "FR-009: {out}");
    assert!(!stack.sesh_ok(&["feed", "list", "ana"]).contains("ferry"));
    let notes = stack.notifications_as("u-ana", "NTFY_PASSWORD_ANA", &ana_topic);
    assert_eq!(notes.len(), 1, "ana is asked");
    assert!(
        notes[0]["message"].as_str().unwrap().contains("ferry.xml"),
        "the approval shows the address: {}",
        notes[0]
    );
    let link = notes[0]["actions"][0]["url"].as_str().unwrap();
    let (page, secret) = link.split_once('#').unwrap();
    assert_eq!(decide(page, secret, "approve"), 200);
    let out = anabot.execute(&add);
    assert!(out.contains("\"added\":\"ferry\""), "{out}");
    assert!(stack.sesh_ok(&["feed", "list", "ana"]).contains("ferry"));
    let out = benbot.execute("async () => await feeds.list({})");
    assert!(
        !out.contains("ferry"),
        "SC-001: ben sees ana's new feed: {out}"
    );
    let out =
        anabot.execute("async () => await feeds.add({ url: \"file:///etc/passwd\", name: \"x\" })");
    assert!(out.contains("only http and https"), "FR-009: {out}");
    assert_eq!(
        stack
            .notifications_as("u-ana", "NTFY_PASSWORD_ANA", &ana_topic)
            .len(),
        1,
        "FR-009: no one was asked about a bad address"
    );

    // US4 (FR-005, SC-002): Miniflux reaches only the internet and its
    // database; only the gateway reaches Miniflux.
    let nets = stack.networks_of("miniflux");
    let mut names: Vec<&str> = nets
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["sesh_feeds", "sesh_feeds-db", "sesh_feeds-egress"],
        "FR-005: Miniflux's networks"
    );
    // Addresses Miniflux must not reach (specs/008: no `default`; the
    // gateway's database network and Postgres's on another service's).
    let ip_on = |service: &str, net: &str| {
        stack.networks_of(service)[format!("sesh_{net}").as_str()]["IPAddress"]
            .as_str()
            .unwrap_or_else(|| panic!("{service} has no address on {net}"))
            .to_string()
    };
    let (gw_ip, pg_ip) = (
        ip_on("gateway", "gateway-db"),
        ip_on("postgres", "litellm-db"),
    );
    // Probes run in Miniflux's network namespace from the gateway image
    // (which has bash), so they don't depend on tools in Miniflux's image.
    let reaches = |target: &str| stack.probe_from("miniflux", target);
    assert!(
        reaches("postgres:5432"),
        "control: Miniflux can't reach its database on feeds-db, so the probe is broken"
    );
    assert!(
        !reaches(&format!("{gw_ip}:7001")),
        "SC-002: Miniflux reached the gateway's executor port on gateway-db"
    );
    assert!(
        !reaches(&format!("{pg_ip}:5432")),
        "SC-002: Miniflux reached Postgres on another service's database network"
    );
    // Its own login reaches only its own database (specs/006 R2).
    let as_miniflux = |db: &str| {
        stack
            .compose(
                &[],
                &[],
                &[
                    "exec", "-T", "postgres", "psql", "-U", "miniflux", "-d", db, "-Atc",
                    "SELECT 1",
                ],
            )
            .status
            .success()
    };
    assert!(
        as_miniflux("miniflux"),
        "Miniflux can't use its own database"
    );
    for db in [
        "gateway",
        "infisical",
        "paperless",
        "openwebui",
        "litellm",
        "sesh",
    ] {
        assert!(
            !as_miniflux(db),
            "R2: the miniflux login reached the {db} database"
        );
    }
    let (ok, out) = stack.bash_in("executor-anabot", "exec 3<>/dev/tcp/miniflux/8080");
    assert!(!ok, "SC-002: an executor reached Miniflux: {out}");
    // `docker compose port` prints a placeholder for an unpublished port,
    // so read the container's port bindings instead.
    let bindings = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{json .HostConfig.PortBindings}}",
            &stack.container("miniflux"),
        ])
        .output()
        .expect("docker inspect");
    let bindings = String::from_utf8_lossy(&bindings.stdout).trim().to_string();
    assert!(
        bindings == "{}" || bindings == "null",
        "FR-005: Miniflux publishes a port: {bindings}"
    );
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "feeds"), "pass", "FR-012: {checks}");

    // Removal stops reads.
    stack.sesh_ok(&["feed", "remove", "ana", "ferry"]);
    let out = anabot.execute("async () => await feeds.latest({ feed: \"ferry\" })");
    assert!(out.contains("no feed named ferry"), "{out}");

    // FR-011: changes and calls are audited, with the owner.
    for reason in [
        "feed.added household/harbour",
        "feed.added ana/garden",
        "feed.removed ana/ferry",
    ] {
        assert_ne!(
            stack.audit_count(&format!("reason = '{reason}'")),
            0,
            "FR-011: no audit row for {reason:?}"
        );
    }
    assert_ne!(
        stack.audit_count(
            "method = 'add' AND outcome = 'allowed' AND reason LIKE 'approved %owner ana%'"
        ),
        0,
        "FR-011: the approved add"
    );
    assert_ne!(
        stack
            .audit_count("connector = 'feeds' AND method = 'latest' AND reason LIKE '%owner ben%'"),
        0
    );

    // SC-004, FR-010: the Miniflux admin password is nowhere but secrets.env.
    let pw = stack.secret("MINIFLUX_ADMIN_PASSWORD");
    let audit = stack.sql(
        "SELECT string_agg(coalesce(reason, '') || ' ' || args_summary::text, ' ') FROM audit",
    );
    let logs = String::from_utf8_lossy(
        &stack
            .compose(
                &[],
                &[],
                &[
                    "logs",
                    "--no-color",
                    "gateway",
                    "executor-anabot",
                    "executor-benbot",
                    "executor-house",
                ],
            )
            .stdout,
    )
    .into_owned();
    let seen: Vec<&String> = anabot
        .seen
        .iter()
        .chain(&benbot.seen)
        .chain(&house.seen)
        .collect();
    for (what, text) in [("the audit log", &audit), ("logs", &logs)]
        .into_iter()
        .chain(seen.into_iter().map(|t| ("a result", t)))
    {
        assert!(
            !text.contains(leak_probe(&pw)),
            "SC-004: the Miniflux password reached {what}"
        );
    }
    for entry in walk(&stack.dir) {
        if entry.file_name().is_some_and(|n| n == "secrets.env") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&entry) {
            assert!(
                !text.contains(&pw),
                "SC-004: the Miniflux password is in {}",
                entry.display()
            );
        }
    }
}

impl Stack {
    /// Whether `service`'s network namespace can open a TCP connection to
    /// `host:port`, probed with bash from the gateway image.
    fn probe_from(&self, service: &str, target: &str) -> bool {
        let id = self.container(service);
        let (host, port) = target.rsplit_once(':').unwrap();
        let image =
            std::env::var("SESH_GATEWAY_IMAGE").unwrap_or_else(|_| "sesh-gateway:dev".into());
        Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                &format!("container:{id}"),
                "--entrypoint",
                "bash",
                &image,
                "-c",
            ])
            .arg(format!(
                "timeout 5 bash -c 'exec 3<>/dev/tcp/{host}/{port}'"
            ))
            .status()
            .expect("docker run")
            .success()
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

// ---- Feature 007: service isolation ----

/// Each adopted service's Postgres login, named like its database.
const SERVICE_ROLES: [&str; 5] = ["openwebui", "litellm", "infisical", "paperless", "miniflux"];
/// The services whose data the pre-007 fixture hands back to the superuser.
const PRE_007_ROLES: [&str; 4] = ["openwebui", "litellm", "infisical", "paperless"];
/// (client, its Redis, the network they alone share).
const REDIS_CLIENTS: [(&str, &str, &str); 3] = [
    ("open-webui", "redis-openwebui", "sesh_openwebui-redis"),
    ("infisical", "redis-infisical", "sesh_infisical-redis"),
    ("paperless", "redis-paperless", "sesh_paperless-redis"),
];

/// Objects in the current database that its owner doesn't own, other
/// than an extension's and array types (which follow their element type).
const NOT_OWNED_SQL: &str = "SELECT kind || ' ' || name || ' (' || pg_get_userbyid(owner) || ')' FROM ( \
  SELECT 'schema' AS kind, n.oid, n.nspname::text AS name, n.nspowner AS owner FROM pg_namespace n \
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
      AND n.nspowner <> 'pg_database_owner'::regrole \
  UNION ALL SELECT 'relation', c.oid, c.oid::regclass::text, c.relowner FROM pg_class c \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
  UNION ALL SELECT 'routine', p.oid, p.oid::regprocedure::text, p.proowner FROM pg_proc p \
    JOIN pg_namespace n ON n.oid = p.pronamespace \
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
  UNION ALL SELECT 'type', t.oid, t.oid::regtype::text, t.typowner FROM pg_type t \
    JOIN pg_namespace n ON n.oid = t.typnamespace \
    WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' AND t.typcategory <> 'A' \
) o WHERE o.owner <> (SELECT datdba FROM pg_database WHERE datname = current_database()) \
  AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.objid = o.oid AND d.deptype = 'e') \
ORDER BY 1 LIMIT 20";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

impl Stack {
    /// psql as `role` over the Postgres container's local socket.
    fn psql_as(&self, role: &str, db: &str, sql: &str) -> (bool, String) {
        let out = self.compose(
            &[],
            &[],
            &[
                "exec",
                "-T",
                "postgres",
                "psql",
                "-U",
                role,
                "-d",
                db,
                "-v",
                "ON_ERROR_STOP=1",
                "-Atc",
                sql,
            ],
        );
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout).trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        )
    }

    /// SQL as the superuser in `db`; panics on failure.
    fn sql_in(&self, db: &str, sql: &str) -> String {
        let (ok, out) = self.psql_as("sesh", db, sql);
        assert!(ok, "{db}: {out}");
        out
    }

    /// A SQL script on psql's stdin, as the superuser in `db`.
    fn sql_script(&self, db: &str, script: &str) {
        use std::io::Write;
        let mut child = self
            .compose_command(&[], &[])
            .args([
                "exec",
                "-T",
                "postgres",
                "psql",
                "-U",
                "sesh",
                "-d",
                db,
                "-q",
                "-v",
                "ON_ERROR_STOP=1",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("run docker compose exec");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "script in {db}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Paperless's API, as its admin, from inside its container (the
    /// password never leaves it).
    fn paperless_api(&self, args: &str) -> (bool, String) {
        self.bash_in(
            "paperless",
            &format!(
                "printf 'user = \"sesh-gateway:%s\"\\n' \"$PAPERLESS_ADMIN_PASSWORD\" | curl -fsS -K - {args}"
            ),
        )
    }

    /// FR-001–FR-003: each login opens its own database and no other, and
    /// has no administrative rights.
    fn assert_logins_isolated(&self, when: &str) {
        let others = ["gateway", "sesh", "postgres"];
        for role in SERVICE_ROLES {
            let (ok, out) = self.psql_as(role, role, "SELECT 1");
            assert!(ok, "{when}: {role} can't open its own database: {out}");
            for db in SERVICE_ROLES.iter().chain(&others).filter(|d| **d != role) {
                let (ok, _) = self.psql_as(role, db, "SELECT 1");
                assert!(!ok, "{when}: SC-001: the {role} login opened {db}");
            }
            assert_eq!(
                self.sql_in(
                    "sesh",
                    &format!(
                        "SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls FROM pg_roles WHERE rolname = '{role}'"
                    )
                ),
                "f",
                "{when}: FR-002: {role} has administrative rights"
            );
        }
        // FR-009: the gateway's login is as it was.
        assert!(self.psql_as("sesh_gateway", "gateway", "SELECT 1").0);
        for db in SERVICE_ROLES.iter().chain(&["sesh", "postgres"]) {
            assert!(!self.psql_as("sesh_gateway", db, "SELECT 1").0, "{db}");
        }
    }

    /// SC-002: every service database, and everything in it, is its role's.
    fn assert_owned(&self, when: &str) {
        for role in SERVICE_ROLES {
            assert_eq!(
                self.sql_in(
                    role,
                    "SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname = current_database()"
                ),
                role,
                "{when}: the {role} database's owner"
            );
            let left = self.sql_in(role, NOT_OWNED_SQL);
            assert!(left.is_empty(), "{when}: in {role}, not {role}'s: {left}");
        }
    }
}

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn isolation_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-isolation-stack-{}", std::process::id()));
    let stack = Stack { dir };
    let kept = format!("cred-{}-kept", std::process::id());
    let replaced = format!("cred-{}-replaced", std::process::id());
    let cred_hash = sha256_hex(&kept);
    let stub_env = [("STUB_TESTCRED_SHA256", cred_hash.as_str())];
    let up = |what: &str| {
        let out = stack.sesh_env(&stub_env, &["up"]);
        assert!(
            out.status.success(),
            "{what}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    stack.sesh_ok(&["init", "--stub", "--admin-email", "admin@sesh.test"]);
    stack.sesh_ok(&["agent", "add", "alice", "--connectors", "testcred"]);
    up("a fresh install");

    // US1 on a fresh install.
    stack.assert_logins_isolated("fresh install");
    stack.assert_owned("fresh install");
    let (_, checks) = stack.doctor();
    assert_eq!(status_of(&checks, "db-isolation"), "pass", "{checks}");

    // US2: back to how an install from before feature 007 ran: the four
    // services stopped, their databases handed back to the superuser with
    // probe objects of every kind, then started with the superuser's
    // connection strings and one shared Redis.
    let legacy = fixture("pre-007.compose.yaml");
    let legacy_pw = sha256_hex(&format!(
        "legacy-redis-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    let legacy_env = [
        ("LEGACY_REDIS_PASSWORD", legacy_pw.as_str()),
        ("STUB_TESTCRED_SHA256", cred_hash.as_str()),
    ];
    let out = stack.compose(
        &[],
        &[],
        &["stop", "open-webui", "litellm", "infisical", "paperless"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stack.sql_script(
        "sesh",
        &std::fs::read_to_string(fixture("pre-007.sql")).unwrap(),
    );
    let objects = std::fs::read_to_string(fixture("pre-007-objects.sql")).unwrap();
    for db in PRE_007_ROLES {
        stack.sql_script(db, &objects);
    }
    // Before feature 008, Postgres let every login in from anywhere (the
    // image's default rule); the superuser connection strings need it. The
    // upgrade's `sesh up` rewrites the rules (specs/008 R10).
    std::fs::write(
        stack.dir.join("postgres-conf/pg_hba.conf"),
        "local all all trust\nhost all all all scram-sha-256\n",
    )
    .unwrap();
    stack.sql_in("sesh", "SELECT pg_reload_conf()");
    let out = stack.compose(
        &[&legacy],
        &legacy_env,
        &[
            "up",
            "-d",
            "--wait",
            "redis",
            "litellm",
            "infisical",
            "paperless",
            "open-webui",
        ],
    );
    assert!(
        out.status.success(),
        "the pre-007 stack didn't start: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for db in PRE_007_ROLES {
        assert_eq!(
            stack.sql_in(
                "sesh",
                &format!("SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname = '{db}'")
            ),
            "sesh",
            "the fixture gave {db} back to the superuser"
        );
        assert_ne!(
            stack.sql_in(
                db,
                "SELECT count(*) FROM pg_tables WHERE tableowner = 'sesh' AND schemaname = 'public'"
            ),
            "0",
            "{db}'s own tables belong to the superuser before the upgrade"
        );
        assert!(
            !stack.psql_as(db, db, "SELECT 1").0,
            "the {db} login is gone before the upgrade"
        );
    }

    // US4 (FR-010, SC-004): doctor fails on the pre-007 stack.
    let (ok, checks) = stack.doctor();
    assert!(!ok, "doctor passed on the pre-007 stack");
    assert_eq!(status_of(&checks, "db-isolation"), "fail", "{checks}");
    let detail = checks
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "db-isolation")
        .unwrap()["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        detail.contains("database litellm is owned by sesh"),
        "{detail}"
    );

    // Data made in the pre-007 state: an Open WebUI user, a router key, a
    // stored credential, a Paperless tag.
    let admin = signin("admin@sesh.test", &stack.secret("WEBUI_ADMIN_PASSWORD"));
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/v1/auths/add"),
        Some(&admin),
        Some(json!({"name": "Before", "email": "before@sesh.test",
            "password": "before-password-1", "role": "user"})),
    );
    assert_eq!(s, 200, "creating a user failed: {body}");
    let (s, body) = call(
        "POST",
        &format!("{ROUTER}/key/generate"),
        Some(&stack.secret("LITELLM_MASTER_KEY")),
        Some(json!({"key_alias": "isolation-before", "models": ["agent-main"]})),
    );
    assert_eq!(s, 200, "creating a router key failed: {body}");
    let before_key = body["key"].as_str().expect("a key").to_string();
    assert_eq!(chat("agent-main", &before_key).0, 200);
    stack.fill_credential("testcred", "TOKEN", &kept);
    let mut alice = Mcp::connect(&stack.token("alice")).expect("alice connects");
    let out = alice.execute("async () => await testcred.check({})");
    assert!(
        out.contains("\"ok\":true"),
        "the pre-007 secret store: {out}"
    );
    let (ok, out) = stack.paperless_api(
        "-H 'Content-Type: application/json' -d '{\"name\":\"isolation-before\"}' http://127.0.0.1:8000/api/tags/",
    );
    assert!(ok, "creating a Paperless tag failed: {out}");

    // US2: the upgrade is `sesh up` alone. A pre-007 secrets.env has none
    // of the new passwords (they were only needed until now to interpolate
    // the current Compose file), so `sesh up` makes them.
    let new_keys = [
        "OPENWEBUI_DB_PASSWORD",
        "LITELLM_DB_PASSWORD",
        "INFISICAL_DB_PASSWORD",
        "PAPERLESS_DB_PASSWORD",
        "REDIS_OPENWEBUI_PASSWORD",
        "REDIS_INFISICAL_PASSWORD",
        "REDIS_PAPERLESS_PASSWORD",
    ];
    let path = stack.dir.join("secrets.env");
    let text = std::fs::read_to_string(&path).unwrap();
    let kept_lines: Vec<&str> = text
        .lines()
        .filter(|l| !new_keys.iter().any(|k| l.starts_with(&format!("{k}="))))
        .collect();
    std::fs::write(&path, kept_lines.join("\n") + "\n").unwrap();
    let printed = up("the upgrade");
    for key in new_keys {
        assert!(
            printed.contains(&format!("generated {key}")),
            "FR-007: {printed}"
        );
    }
    stack.assert_owned("after the upgrade");
    for db in PRE_007_ROLES {
        assert_eq!(
            stack.sql_in(db, "SELECT body FROM sesh_probe.notes"),
            "made before 007",
            "SC-002: {db}'s probe row"
        );
        let (ok, out) = stack.psql_as(
            db,
            db,
            "INSERT INTO sesh_probe.notes (mood, body) VALUES ('ok', 'after'); \
             CREATE OR REPLACE VIEW sesh_probe.recent AS SELECT * FROM sesh_probe.notes; \
             CREATE OR REPLACE FUNCTION public.sesh_probe_fn() RETURNS int LANGUAGE sql AS 'SELECT 2'; \
             REFRESH MATERIALIZED VIEW sesh_probe.counts; SELECT n FROM sesh_probe.counts",
        );
        assert!(
            ok && out.ends_with('2'),
            "{db}'s login can't change its own objects: {out}"
        );
    }
    let gone = Command::new("docker")
        .args([
            "ps",
            "-aq",
            "--filter",
            "label=com.docker.compose.project=sesh",
            "--filter",
            "label=com.docker.compose.service=redis",
        ])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&gone.stdout).trim().is_empty(),
        "the shared Redis is still there after the upgrade"
    );

    // The services work, with their earlier data.
    let admin = signin("admin@sesh.test", &stack.secret("WEBUI_ADMIN_PASSWORD"));
    signin("before@sesh.test", "before-password-1");
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/chat/completions"),
        Some(&admin),
        Some(json!({"model": "agent-main", "stream": false,
            "messages": [{"role": "user", "content": "hello"}]})),
    );
    assert_eq!(s, 200, "Open WebUI chat after the upgrade: {body}");
    assert_eq!(
        chat("agent-main", &before_key).0,
        200,
        "the earlier router key"
    );
    assert_eq!(chat("agent-main", &stack.secret("WEBUI_ROUTER_KEY")).0, 200);
    let mut alice = Mcp::connect(&stack.token("alice")).expect("alice connects");
    let out = alice.execute("async () => await testcred.check({})");
    assert!(out.contains("\"ok\":true"), "the stored credential: {out}");
    stack.fill_credential("testcred", "TOKEN", &replaced);
    let out = alice.execute("async () => await testcred.check({})");
    assert!(
        out.contains("\"ok\":false"),
        "a credential written after the upgrade is read back: {out}"
    );
    let (ok, out) =
        stack.paperless_api("'http://127.0.0.1:8000/api/tags/?name__iexact=isolation-before'");
    assert!(
        ok && out.contains("isolation-before"),
        "the earlier tag: {out}"
    );
    let (ok, out) = stack.paperless_api(
        "-H 'Content-Type: application/json' -d '{\"name\":\"isolation-after\"}' http://127.0.0.1:8000/api/tags/",
    );
    assert!(ok, "Paperless can't write after the upgrade: {out}");
    let (_, checks) = stack.doctor();
    for id in [
        "db-isolation",
        "services-healthy",
        "paperless",
        "credentials",
    ] {
        assert_eq!(
            status_of(&checks, id),
            "pass",
            "{id} after the upgrade: {checks}"
        );
    }

    // US2: a second `sesh up` changes nothing.
    let again = up("a second run");
    assert!(!again.contains("generated "), "{again}");
    stack.assert_owned("after a second run");

    // US1 after the upgrade.
    stack.assert_logins_isolated("after the upgrade");

    // US3 (FR-008, SC-003): each service reaches only its own Redis.
    for (_, redis, network) in REDIS_CLIENTS {
        let nets = stack.networks_of(redis);
        let names: Vec<&str> = nets
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(names, vec![network], "{redis}'s networks");
        let out = stack.compose(&[], &[], &["exec", "-T", redis, "redis-cli", "ping"]);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            text.contains("NOAUTH"),
            "{redis} answers without its password: {text}"
        );
    }
    let address = |redis: &str, network: &str| {
        stack.networks_of(redis)[network]["IPAddress"]
            .as_str()
            .unwrap_or_else(|| panic!("{redis} has no address on {network}"))
            .to_string()
    };
    for (client, own, _) in REDIS_CLIENTS {
        assert!(
            stack.probe_from(client, &format!("{own}:6379")),
            "control: {client} can't reach {own}, so the probe is broken"
        );
        for (_, other, network) in REDIS_CLIENTS.iter().filter(|(_, r, _)| *r != own) {
            assert!(
                !stack.probe_from(client, &format!("{other}:6379")),
                "SC-003: {client} reached {other} by name"
            );
            assert!(
                !stack.probe_from(client, &format!("{}:6379", address(other, network))),
                "SC-003: {client} reached {other} by address"
            );
        }
    }
    for (_, redis, network) in REDIS_CLIENTS {
        assert!(
            !stack.probe_from("gateway", &format!("{}:6379", address(redis, network))),
            "the gateway reached {redis}"
        );
    }

    // SC-005: no login password outside secrets.env, sesh's output or the
    // Postgres and gateway logs.
    let logs = String::from_utf8_lossy(
        &stack
            .compose(&[], &[], &["logs", "--no-color", "postgres", "gateway"])
            .stdout,
    )
    .into_owned();
    for key in [
        "OPENWEBUI_DB_PASSWORD",
        "LITELLM_DB_PASSWORD",
        "INFISICAL_DB_PASSWORD",
        "PAPERLESS_DB_PASSWORD",
        "MINIFLUX_DB_PASSWORD",
        "REDIS_OPENWEBUI_PASSWORD",
        "REDIS_INFISICAL_PASSWORD",
        "REDIS_PAPERLESS_PASSWORD",
    ] {
        let pw = stack.secret(key);
        assert!(!logs.contains(&pw), "SC-005: {key} is in the logs");
        assert!(
            !printed.contains(&pw) && !again.contains(&pw),
            "SC-005: sesh up printed {key}"
        );
        for entry in walk(&stack.dir) {
            if entry.file_name().is_some_and(|n| n == "secrets.env") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&entry) {
                assert!(
                    !text.contains(&pw),
                    "SC-005: {key} is in {}",
                    entry.display()
                );
            }
        }
    }
}

// ---- Feature 008: network isolation ----

impl Stack {
    /// Every address a service has, on any of its networks.
    fn addresses_of(&self, service: &str) -> Vec<String> {
        let nets = self.networks_of(service);
        nets.as_object()
            .map(|m| {
                m.values()
                    .filter_map(|v| v["IPAddress"].as_str())
                    .filter(|a| !a.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Probes every `(host, port)` from `service`'s network namespace at
    /// once (one container, probes in parallel). Returns which connected.
    fn probe_many(&self, service: &str, targets: &[(String, u16)]) -> Vec<bool> {
        let id = self.container(service);
        assert!(!id.is_empty(), "{service} isn't running");
        let mut script = String::new();
        for (i, (host, port)) in targets.iter().enumerate() {
            script.push_str(&format!(
                "(timeout 3 bash -c 'exec 3<>/dev/tcp/{host}/{port}' 2>/dev/null && echo ok {i} || echo no {i}) &\n"
            ));
        }
        script.push_str("wait\n");
        let image =
            std::env::var("SESH_GATEWAY_IMAGE").unwrap_or_else(|_| "sesh-gateway:dev".into());
        let out = Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                &format!("container:{id}"),
                "--entrypoint",
                "bash",
                &image,
                "-c",
                &script,
            ])
            .output()
            .expect("docker run");
        let text = String::from_utf8_lossy(&out.stdout);
        let mut ok = vec![false; targets.len()];
        for line in text.lines() {
            if let Some(i) = line
                .strip_prefix("ok ")
                .and_then(|i| i.trim().parse::<usize>().ok())
            {
                ok[i] = true;
            }
        }
        ok
    }

    /// `psql` from `service`'s network namespace as `role`, with the
    /// Postgres image's client. Returns (connected, stderr).
    fn psql_from(&self, service: &str, role: &str, db: &str, password: &str) -> (bool, String) {
        let id = self.container(service);
        let image = String::from_utf8_lossy(
            &Command::new("docker")
                .args([
                    "inspect",
                    "-f",
                    "{{.Config.Image}}",
                    &self.container("postgres"),
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let out = Command::new("docker")
            .args(["run", "--rm", "--network", &format!("container:{id}"), "-e"])
            .arg(format!("PGPASSWORD={password}"))
            .args([
                &image, "psql", "-h", "postgres", "-U", role, "-d", db, "-Atc", "SELECT 1",
            ])
            .output()
            .expect("docker run psql");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

/// (role, database, the service that uses it, its password key).
const DB_ROLES: [(&str, &str, &str, &str); 6] = [
    ("litellm", "litellm", "litellm", "LITELLM_DB_PASSWORD"),
    (
        "openwebui",
        "openwebui",
        "open-webui",
        "OPENWEBUI_DB_PASSWORD",
    ),
    ("sesh_gateway", "gateway", "gateway", "GATEWAY_DB_PASSWORD"),
    (
        "infisical",
        "infisical",
        "infisical",
        "INFISICAL_DB_PASSWORD",
    ),
    (
        "paperless",
        "paperless",
        "paperless",
        "PAPERLESS_DB_PASSWORD",
    ),
    ("miniflux", "miniflux", "miniflux", "MINIFLUX_DB_PASSWORD"),
];

/// The agent `network_stack` adds; its executor's network joins the gateway.
const NET_AGENT: &str = "alice";

impl Stack {
    /// Whether `docker network inspect sesh_<name>` finds the network.
    fn network_exists(&self, name: &str) -> bool {
        Command::new("docker")
            .args(["network", "inspect", &format!("sesh_{name}")])
            .output()
            .unwrap()
            .status
            .success()
    }

    /// The doctor's `network-layout` check: (status, detail).
    fn network_layout_check(&self) -> (String, String) {
        let (_, checks) = self.doctor();
        let check = checks
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "network-layout")
            .unwrap_or_else(|| panic!("no network-layout check: {checks}"))
            .clone();
        (
            check["status"].as_str().unwrap_or_default().to_string(),
            check["detail"].as_str().unwrap_or_default().to_string(),
        )
    }

    /// FR-001 to FR-008: every service is on exactly its networks, and
    /// `sesh_default` is gone.
    fn assert_network_layout(&self, when: &str) {
        for s in layout::services(true) {
            let mut want: Vec<String> = layout::networks_of(s, true)
                .iter()
                .map(|n| format!("sesh_{n}"))
                .collect();
            if s == "gateway" {
                want.push(format!("sesh_exec-{NET_AGENT}"));
            }
            want.sort();
            let mut got: Vec<String> = self
                .networks_of(s)
                .as_object()
                .unwrap_or_else(|| panic!("{when}: {s} has no networks"))
                .keys()
                .cloned()
                .collect();
            got.sort();
            assert_eq!(got, want, "{when}: FR-002..008: {s}'s networks");
        }
        assert!(
            !self.network_exists("default"),
            "{when}: FR-001: sesh_default exists"
        );
    }

    /// FR-014, SC-001: the reachability matrix, probed by every address.
    fn assert_reachability(&self, when: &str) {
        let executor = format!("executor-{NET_AGENT}");
        let mut sources: Vec<&str> = layout::services(true);
        sources.push(&executor);
        let addrs: std::collections::BTreeMap<&str, Vec<String>> =
            sources.iter().map(|s| (*s, self.addresses_of(s))).collect();
        let mut wrong = vec![];
        for from in &sources {
            let mut targets = vec![];
            let mut expect = vec![];
            for (to, port) in layout::LISTENS {
                if to == from || !addrs.contains_key(to) {
                    continue;
                }
                let want = if *from == executor {
                    *to == "gateway" && *port == 7001
                } else {
                    layout::reachable(from, to, *port, true)
                };
                for a in &addrs[to] {
                    targets.push((a.clone(), *port));
                    expect.push((*to, *port, want));
                }
            }
            let got = self.probe_many(from, &targets);
            // A destination counts as reachable if any of its addresses is.
            let mut seen: std::collections::BTreeMap<(&str, u16), (bool, bool)> =
                Default::default();
            for ((to, port, want), ok) in expect.into_iter().zip(got) {
                let e = seen.entry((to, port)).or_insert((want, false));
                e.1 |= ok;
            }
            for ((to, port), (want, ok)) in seen {
                if want != ok {
                    wrong.push(format!(
                        "{from} -> {to}:{port}: expected {}, got {}",
                        if want { "open" } else { "closed" },
                        if ok { "open" } else { "closed" }
                    ));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "{when}: FR-014: reachability differs from the layout:\n{}",
            wrong.join("\n")
        );
    }

    /// FR-018 to FR-020, SC-007: each database login only from its own
    /// network, the superuser never over the network, and the active rules
    /// are the networks' actual ranges.
    fn assert_db_logins(&self, when: &str) {
        let pg_pw = self.secret("POSTGRES_PASSWORD");
        for (role, db, svc, key) in DB_ROLES {
            let pw = self.secret(key);
            let (ok, err) = self.psql_from(svc, role, db, &pw);
            assert!(ok, "{when}: {role} can't log in from {svc}: {err}");
            for (_, _, other, _) in DB_ROLES.iter().filter(|(_, _, o, _)| *o != svc) {
                let (ok, err) = self.psql_from(other, role, db, &pw);
                assert!(!ok, "{when}: FR-018: {role} logged in from {other}");
                assert!(
                    err.contains("pg_hba.conf"),
                    "{when}: refused by the login rules, not something else: {err}"
                );
            }
            let (ok, err) = self.psql_from(svc, "sesh", "sesh", &pg_pw);
            assert!(
                !ok && err.contains("pg_hba.conf"),
                "{when}: FR-019: the superuser logged in from {svc}: {err}"
            );
        }
        let rules = self.sql("SELECT coalesce(database[1], '') || ' ' || coalesce(user_name[1], '') || ' ' || coalesce(address, '') || '/' || coalesce(netmask, '') FROM pg_hba_file_rules WHERE type = 'host' AND auth_method = 'scram-sha-256' ORDER BY 1");
        for line in rules.lines() {
            assert!(
                DB_ROLES
                    .iter()
                    .any(|(role, db, _, _)| line.starts_with(&format!("{db} {role} "))),
                "{when}: FR-020: a network rule for no service login: {line}"
            );
        }
        for (role, db, _, _) in DB_ROLES {
            let (_, _, net) = layout::DB_LOGINS
                .iter()
                .find(|(r, _, _)| *r == role)
                .unwrap();
            let subnet = String::from_utf8_lossy(
                &Command::new("docker")
                    .args([
                        "network",
                        "inspect",
                        "-f",
                        "{{range .IPAM.Config}}{{.Subnet}} {{end}}",
                        &format!("sesh_{net}"),
                    ])
                    .output()
                    .unwrap()
                    .stdout,
            )
            .trim()
            .to_string();
            let prefix = subnet.split('/').next().unwrap();
            assert!(
                rules
                    .lines()
                    .any(|l| l.starts_with(&format!("{db} {role} {prefix}/"))),
                "{when}: FR-020: no rule for {role} on {subnet}: {rules}"
            );
        }
        let last = self.sql("SELECT type || ' ' || auth_method FROM pg_hba_file_rules ORDER BY line_number DESC LIMIT 1");
        assert_eq!(
            last, "host reject",
            "{when}: the last rule rejects everything else"
        );
    }
}

/// The services that were on the shared `default` network before feature
/// 008 (`tests/fixtures/pre-008.compose.yaml`).
const PRE_008_DEFAULT: [&str; 8] = [
    "postgres",
    "litellm",
    "open-webui",
    "gateway",
    "infisical",
    "ntfy",
    "paperless",
    "stub-llm",
];

#[test]
#[ignore = "needs Docker and the sesh images; run with `make stack-test`"]
fn network_stack() {
    let dir = std::env::temp_dir().join(format!("sesh-network-stack-{}", std::process::id()));
    let stack = Stack { dir };
    let cred = format!("cred-{}-network", std::process::id());
    let cred_hash = sha256_hex(&cred);
    let stub_env = [("STUB_TESTCRED_SHA256", cred_hash.as_str())];
    let up = |what: &str| {
        let out = stack.sesh_env(&stub_env, &["up"]);
        assert!(
            out.status.success(),
            "{what}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    stack.sesh_ok(&["init", "--stub", "--admin-email", "admin@sesh.test"]);
    stack.sesh_ok(&["agent", "add", NET_AGENT, "--connectors", "search,testcred"]);
    up("a fresh install");

    // US1 on a fresh install.
    stack.assert_network_layout("fresh install");
    stack.assert_reachability("fresh install");
    stack.assert_db_logins("fresh install");
    let (status, detail) = stack.network_layout_check();
    assert_eq!(status, "pass", "fresh install: {detail}");

    // US5 (FR-013, T019): a container attached to an extra network by hand fails
    // the doctor, which names the service and the network; detaching it
    // passes again.
    let webui = stack.container("open-webui");
    let attach = |verb: &str| {
        let out = Command::new("docker")
            .args(["network", verb, "sesh_secrets", &webui])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docker network {verb}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    attach("connect");
    let (status, detail) = stack.network_layout_check();
    assert_eq!(status, "fail", "US5: the drift isn't reported: {detail}");
    assert!(
        detail.contains("open-webui") && detail.contains("secrets"),
        "US5: the detail names the service and the network: {detail}"
    );
    attach("disconnect");
    let (status, detail) = stack.network_layout_check();
    assert_eq!(status, "pass", "US5: after disconnecting: {detail}");

    // US3 (T015): back to how an install from before feature 008 ran: the
    // services that were on the shared `default` network back on it, and
    // Postgres letting every login in from anywhere (the image's default
    // rule). The rules go first: on `default`, a service reaches Postgres
    // from its `default` address, which the current rules refuse, so the
    // pre-008 stack can't start under them. The recreated Postgres reads
    // the file from its mounted directory.
    std::fs::write(
        stack.dir.join("postgres-conf/pg_hba.conf"),
        "local all all trust\nhost all all all scram-sha-256\n",
    )
    .unwrap();
    stack.sql_in("sesh", "SELECT pg_reload_conf()");
    let out = stack.compose(
        &[&fixture("pre-008.compose.yaml")],
        &stub_env,
        &["up", "-d", "--wait"],
    );
    assert!(
        out.status.success(),
        "the pre-008 stack didn't start: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for s in PRE_008_DEFAULT {
        assert!(
            stack.networks_of(s).get("sesh_default").is_some(),
            "pre-008: {s} isn't on sesh_default"
        );
    }
    let (status, _) = stack.network_layout_check();
    assert_eq!(status, "fail", "the doctor passed on the pre-008 layout");

    // Data made in the pre-008 state: an Open WebUI user, a router key, a
    // stored credential, a Paperless tag.
    let admin = signin("admin@sesh.test", &stack.secret("WEBUI_ADMIN_PASSWORD"));
    let (s, body) = call(
        "POST",
        &format!("{WEBUI}/api/v1/auths/add"),
        Some(&admin),
        Some(json!({"name": "Before", "email": "before@sesh.test",
            "password": "before-password-1", "role": "user"})),
    );
    assert_eq!(s, 200, "creating a user failed: {body}");
    let (s, body) = call(
        "POST",
        &format!("{ROUTER}/key/generate"),
        Some(&stack.secret("LITELLM_MASTER_KEY")),
        Some(json!({"key_alias": "network-before", "models": ["agent-main"]})),
    );
    assert_eq!(s, 200, "creating a router key failed: {body}");
    let before_key = body["key"].as_str().expect("a key").to_string();
    assert_eq!(chat("agent-main", &before_key).0, 200);
    stack.fill_credential("testcred", "TOKEN", &cred);
    let mut agent = Mcp::connect(&stack.token(NET_AGENT)).expect("the agent connects");
    let out = agent.execute("async () => await testcred.check({})");
    assert!(
        out.contains("\"ok\":true"),
        "the pre-008 secret store: {out}"
    );
    let (ok, out) = stack.paperless_api(
        "-H 'Content-Type: application/json' -d '{\"name\":\"network-before\"}' http://127.0.0.1:8000/api/tags/",
    );
    assert!(ok, "creating a Paperless tag failed: {out}");

    // US3 (FR-012): the upgrade is `sesh up` alone. Compose recreates
    // every container whose networks changed, so none is left on
    // sesh_default and `sesh up` removes it.
    let printed = up("the upgrade");
    assert!(
        printed.contains("removed the old shared network sesh_default"),
        "FR-012: {printed}"
    );
    assert!(
        !printed.contains("still used by"),
        "FR-012: a container stayed on sesh_default: {printed}"
    );
    assert!(
        printed.contains("database login rules updated"),
        "R10: the permissive login rules are replaced: {printed}"
    );

    // The data is still there.
    signin("before@sesh.test", "before-password-1");
    assert_eq!(
        chat("agent-main", &before_key).0,
        200,
        "the earlier router key"
    );
    let mut agent = Mcp::connect(&stack.token(NET_AGENT)).expect("the agent connects");
    let out = agent.execute("async () => await testcred.check({})");
    assert!(out.contains("\"ok\":true"), "the stored credential: {out}");
    let (ok, out) =
        stack.paperless_api("'http://127.0.0.1:8000/api/tags/?name__iexact=network-before'");
    assert!(
        ok && out.contains("network-before"),
        "the earlier tag: {out}"
    );

    // And the layout, reachability and login rules are the new ones.
    stack.assert_network_layout("after the upgrade");
    stack.assert_reachability("after the upgrade");
    stack.assert_db_logins("after the upgrade");
    let (status, detail) = stack.network_layout_check();
    assert_eq!(status, "pass", "after the upgrade: {detail}");

    // FR-012: a second `sesh up` prints nothing new about networks or
    // login rules.
    let again = up("a second run");
    for news in ["sesh_default", "network", "login rules", "warning:"] {
        assert!(
            !again
                .lines()
                .filter(|l| !l.starts_with("stack is up"))
                .any(|l| l.contains(news)),
            "FR-012: the second run printed {news:?}: {again}"
        );
    }
    stack.assert_network_layout("after a second run");
}
