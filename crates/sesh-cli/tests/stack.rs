//! Stack tests for features 001 (specs/001-foundation/tasks.md T012–T023)
//! and 002 (specs/002-gateway-core/tasks.md T015–T022).
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
        cmd.args(args).output().expect("run docker compose")
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
        msg["result"]["content"]
            .as_array()
            .unwrap_or_else(|| panic!("no content in {msg}"))
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
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
    // The test-only approval-gated connector (contracts/connectors.md); the
    // gateway reloads agents.toml when it changes.
    let agents_file = stack.dir.join("gateway/agents.toml");
    let text = std::fs::read_to_string(&agents_file).unwrap();
    std::fs::write(
        &agents_file,
        text.replacen(
            "[\"search\", \"geo\"]",
            "[\"search\", \"geo\", \"testgate\"]",
            1,
        ),
    )
    .unwrap();
    stack.sesh_ok(&["up"]);
    let (alice_t, bob_t) = (stack.token("alice"), stack.token("bob"));

    // US1 / T015 (FR-001, SC-001): one script calls two connectors.
    let mut alice = Mcp::connect(&alice_t).expect("alice connects");
    let found = alice.tool("search", json!({"query": "weather forecast"}));
    assert!(found.contains("geo.forecast"), "search: {found}");
    let described = alice.tool("describe", json!({"target": "search.web"}));
    assert!(described.contains("query"), "describe: {described}");
    let out = alice.execute(
        "async () => { const w = await search.web({ query: \"oslo\" }); const f = await geo.forecast({}); return { w, f }; }",
    );
    assert!(out.contains("\"completed\""), "US1: {out}");
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

    // US2 / T016 (FR-008): an approval-gated method is refused, whatever
    // the script claims.
    let out = alice.execute("async () => await testgate.act({ approved: true })");
    assert!(out.contains("approval_unavailable"), "FR-008: {out}");

    // US2 / T016 (FR-009): an argument outside the schema is refused.
    let out = alice.execute("async () => await search.web({ query: \"x\", user: \"bob\" })");
    assert!(!out.contains("\"completed\""), "FR-009: {out}");

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
    assert!(took < Duration::from_secs(37), "SC-004: took {took:?}");
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
        !log.contains(&alice_t) && !log.contains(&bob_t),
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
