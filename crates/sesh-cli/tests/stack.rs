//! Stack tests for feature 001 (specs/001-foundation/tasks.md T012–T023).
//!
//! They start the real stack with the stub model server, so they need
//! Docker and the `sesh-stub-llm:dev` image, and are ignored by default:
//!
//!     docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
//!     make stack-test
//!
//! One test runs the whole sequence, because starting the stack is slow.
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
#[ignore = "needs Docker and the sesh-stub-llm:dev image; run with `make stack-test`"]
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
