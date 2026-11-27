//! `sesh eval` (specs/003-model-evals/contracts/cli.md; ADR 0006): runs the
//! suite against a router alias through a separate, short-lived eval
//! gateway, and keeps the results.

pub mod run_loop;
pub mod score;
pub mod store;
pub mod suite;

use crate::config::Config;
use crate::host::compose::{Compose, DockerCompose};
use crate::host::http::{Http, UreqHttp};
use crate::host::mcp::{HttpMcp, Mcp};
use crate::secrets::{self, Secrets};
use anyhow::{Context, Result, bail};
use score::{AuditEntry, TaskResult};
use serde_json::{Value, json};
use sesh_core::{Agent, AgentsFile, GatewaySettings, Home, token_hash};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::Run;
use suite::{Limits, SUITE_VERSION};

pub const EVAL_COMPOSE: &str = include_str!("../../../../deploy/eval/compose.yaml");
pub const EVAL_PROJECT: &str = "sesh-eval";
pub const EVAL_PORT: u16 = 7100;
pub const EVAL_APPROVALS_PORT: u16 = 7102;
pub const EVAL_NTFY_PORT: u16 = 7190;
pub const EVAL_TOPIC: &str = "sesh-eval-approvals";
pub const EVAL_AGENT: &str = "eval";
pub const EVAL_CONNECTORS: [&str; 5] = ["search", "geo", "testgate", "mail", "docs"];
const FAKES: &str = "http://fakes:8000";

/// A missing prerequisite: exit code 3 (contracts/cli.md).
#[derive(Debug)]
pub struct Prerequisite(pub String);

impl std::fmt::Display for Prerequisite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Prerequisite {}

fn prereq(msg: impl Into<String>) -> anyhow::Error {
    Prerequisite(msg.into()).into()
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub alias: String,
    pub threshold: Option<usize>,
    pub turns: Option<u32>,
    pub tool_calls: Option<u32>,
    pub timeout: Option<u64>,
    pub json: bool,
}

/// The CLI's limit flags over a task's defaults (FR-012).
pub fn limits(base: &Limits, o: &Options) -> Limits {
    Limits {
        turns: o.turns.unwrap_or(base.turns),
        tool_calls: o.tool_calls.unwrap_or(base.tool_calls),
        seconds: o.timeout.unwrap_or(base.seconds),
    }
}

/// `server` without any `user:pass@` part.
pub fn strip_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    format!("{scheme}://{host}{path}")
}

fn run_dir(dir: &Path) -> PathBuf {
    store::evals_dir(dir).join(".run")
}

#[cfg(unix)]
fn mode(path: &Path, m: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(m))?;
    Ok(())
}

#[cfg(not(unix))]
fn mode(_: &Path, _: u32) -> Result<()> {
    Ok(())
}

/// The eval gateway's settings: every upstream is the fakes (research.md R2).
pub fn eval_gateway(config: &Config) -> GatewaySettings {
    let mut g = GatewaySettings::default();
    for u in [
        &mut g.upstreams.searxng,
        &mut g.upstreams.kiwix,
        &mut g.upstreams.open_meteo,
        &mut g.upstreams.nominatim,
        &mut g.upstreams.test,
        &mut g.upstreams.paperless,
    ] {
        *u = FAKES.into();
    }
    g.upstreams.google_auth = format!("{FAKES}/fake/google/auth");
    g.upstreams.google_token = format!("{FAKES}/fake/google/token");
    g.upstreams.google_revoke = format!("{FAKES}/fake/google/revoke");
    for u in [
        &mut g.upstreams.gmail,
        &mut g.upstreams.calendar,
        &mut g.upstreams.people,
    ] {
        *u = format!("{FAKES}/fake/google");
    }
    g.ntfy_topic = EVAL_TOPIC.into();
    g.approvals_base_url = format!("http://127.0.0.1:{EVAL_APPROVALS_PORT}");
    g.home = Some(config.gateway.home.clone().unwrap_or(Home {
        latitude: 59.91,
        longitude: 10.75,
    }));
    g
}

/// The run's files and the eval approval channel's passwords.
pub struct Prepared {
    pub audit: PathBuf,
    pub ntfy_gateway_pw: String,
    pub ntfy_person_pw: String,
    pub admin_token: String,
}

/// Writes the run's files; only the token's hash reaches the gateway
/// (data-model.md Eval agent).
pub fn prepare(dir: &Path, config: &Config, token: &str) -> Result<Prepared> {
    let root = store::evals_dir(dir);
    secrets::ensure_private_dir(&root)?;
    let rdir = run_dir(dir);
    let _ = std::fs::remove_dir_all(&rdir);
    let gdir = rdir.join("gateway");
    let adir = rdir.join("audit");
    std::fs::create_dir_all(&gdir)?;
    std::fs::create_dir_all(&adir)?;
    mode(&rdir, 0o755)?;
    mode(&gdir, 0o755)?;
    // The gateway's uid must write here; the evals dir (0700) keeps other
    // users out (research.md R5).
    mode(&adir, 0o777)?;
    AgentsFile {
        agents: vec![Agent {
            owner: sesh_core::HOUSEHOLD.into(),
            name: EVAL_AGENT.into(),
            token_sha256: token_hash(token),
            connectors: EVAL_CONNECTORS.iter().map(|c| c.to_string()).collect(),
        }],
    }
    .save(&gdir)?;
    std::fs::write(
        gdir.join("gateway.toml"),
        toml::to_string_pretty(&eval_gateway(config))?,
    )?;
    for f in ["agents.toml", "gateway.toml"] {
        mode(&gdir.join(f), 0o644)?;
    }
    std::fs::write(rdir.join("compose.yaml"), EVAL_COMPOSE)?;
    std::fs::write(rdir.join("empty.env"), "")?;
    let (gw_pw, person_pw) = (secrets::random_token()?, secrets::random_token()?);
    crate::ntfy::write_config(
        &rdir,
        &[EVAL_TOPIC.to_string()],
        &gw_pw,
        &[crate::ntfy::Reader {
            user: crate::ntfy::PERSON_USER.into(),
            password: person_pw.clone(),
            topics: vec![EVAL_TOPIC.into()],
        }],
    )?;
    std::fs::rename(rdir.join(crate::ntfy::FILE), rdir.join("ntfy-server.yml"))?;
    mode(&rdir.join("ntfy-server.yml"), 0o644)?;
    Ok(Prepared {
        audit: adir.join("audit.jsonl"),
        ntfy_gateway_pw: gw_pw,
        ntfy_person_pw: person_pw,
        admin_token: secrets::random_token()?,
    })
}

/// Answers the eval gateway's approval requests for the person
/// (research.md R8): reads new notifications on the eval topic and posts
/// `decision` to each one's approval page. Returns how many it answered.
pub struct Approver {
    person_pw: String,
    seen: std::cell::RefCell<Vec<String>>,
}

impl Approver {
    pub fn new(person_pw: &str) -> Self {
        Approver {
            person_pw: person_pw.into(),
            seen: Default::default(),
        }
    }

    pub fn answer(&self, approve: bool) -> usize {
        use base64::Engine;
        let basic = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!(
                "{}:{}",
                crate::ntfy::PERSON_USER,
                self.person_pw
            ))
        );
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .build()
            .into();
        let mut answered = 0;
        // Notifications can lag the call that caused them by a moment.
        for _ in 0..10 {
            let Ok(mut r) = agent
                .get(&format!(
                    "http://127.0.0.1:{EVAL_NTFY_PORT}/{EVAL_TOPIC}/json?poll=1&since=all"
                ))
                .header("Authorization", &basic)
                .call()
            else {
                break;
            };
            let text = r.body_mut().read_to_string().unwrap_or_default();
            for line in text.lines() {
                let Ok(m) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                let (Some(id), Some(link)) = (m["id"].as_str(), m["actions"][0]["url"].as_str())
                else {
                    continue;
                };
                if self.seen.borrow().iter().any(|s| s == id) {
                    continue;
                }
                self.seen.borrow_mut().push(id.to_string());
                let Some((page, secret)) = link.split_once('#') else {
                    continue;
                };
                let decision = if approve { "approve" } else { "deny" };
                if let Ok(resp) = agent
                    .post(page)
                    .send_json(json!({"secret": secret, "decision": decision}))
                    && resp.status() == 200
                    && approve
                {
                    answered += 1;
                }
            }
            if answered > 0 || !approve {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        answered
    }
}

pub fn eval_compose(dir: &Path) -> DockerCompose {
    eval_compose_with(dir, "", "")
}

/// Connects the eval agent's owner (the Household) to fake Google through
/// the real flow: the client credentials by one-time form, then the
/// consent link, with fake Google's code (specs/005 research.md R8).
pub fn connect_fake_google(http: &dyn Http, admin_token: &str) -> Result<()> {
    let base = format!("http://127.0.0.1:{EVAL_APPROVALS_PORT}");
    let link = |r: crate::host::http::Reply, what: &str| -> Result<(String, String)> {
        let url = r.body["url"]
            .as_str()
            .with_context(|| format!("no {what} link from the eval gateway ({})", r.status))?;
        let (page, secret) = url.split_once('#').context("a link without a secret")?;
        Ok((page.to_string(), secret.to_string()))
    };
    for (name, value) in [
        ("CLIENT_ID", "eval-client"),
        ("CLIENT_SECRET", "eval-client-secret"),
    ] {
        let r = http.post(
            &format!("{base}/admin/forms"),
            Some(admin_token),
            &json!({"connector": "google", "name": name}),
        )?;
        let (page, secret) = link(r, "form")?;
        let r = http.post(&page, None, &json!({"secret": secret, "value": value}))?;
        if r.status != 200 {
            bail!("filling the eval's {name} failed ({})", r.status);
        }
    }
    let r = http.post(
        &format!("{base}/admin/google/consent"),
        Some(admin_token),
        &json!({"owner": sesh_core::HOUSEHOLD}),
    )?;
    let (page, secret) = link(r, "consent")?;
    let r = http.post(&page, None, &json!({"secret": secret}))?;
    let auth = r.body["redirect"]
        .as_str()
        .with_context(|| format!("no consent redirect ({})", r.status))?;
    let state = auth
        .split("state=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .context("no state in the consent redirect")?
        .replace("%2E", ".");
    let r = http.get(
        &format!("{base}/google/callback?state={state}&code=fake-household"),
        None,
    )?;
    if r.status != 200 {
        bail!("fake Google consent failed ({})", r.status);
    }
    Ok(())
}

/// The eval project, with the eval gateway's ntfy password for `up`.
pub fn eval_compose_with(dir: &Path, ntfy_gateway_pw: &str, admin_token: &str) -> DockerCompose {
    let rdir = run_dir(dir);
    let mut vars = vec![
        ("SESH_EVAL_ADMIN_TOKEN".to_string(), admin_token.to_string()),
        (
            "SESH_EVAL_NTFY_GATEWAY_PASSWORD".to_string(),
            ntfy_gateway_pw.to_string(),
        ),
        ("SESH_EVAL_DIR".to_string(), rdir.display().to_string()),
        ("SESH_EVAL_PORT".to_string(), EVAL_PORT.to_string()),
        (
            "SESH_EVAL_APPROVALS_PORT".to_string(),
            EVAL_APPROVALS_PORT.to_string(),
        ),
        (
            "SESH_EVAL_NTFY_PORT".to_string(),
            EVAL_NTFY_PORT.to_string(),
        ),
    ];
    for v in [
        "SESH_GATEWAY_IMAGE",
        "SESH_EXECUTOR_IMAGE",
        "SESH_STUB_IMAGE",
    ] {
        if let Ok(x) = std::env::var(v) {
            vars.push((v.to_string(), x));
        }
    }
    DockerCompose {
        project: EVAL_PROJECT.into(),
        files: vec![rdir.join("compose.yaml")],
        env_file: rdir.join("empty.env"),
        vars,
    }
}

/// Audit entries appended since `offset` bytes; returns them and the new length.
pub fn read_audit(path: &Path, offset: usize) -> (Vec<AuditEntry>, usize) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let new = text.get(offset..).unwrap_or("");
    let entries = new
        .lines()
        .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok())
        .collect();
    (entries, text.len())
}

/// `evals/.lock`: one run at a time.
pub struct Lock(PathBuf);

impl Lock {
    pub fn take(dir: &Path) -> Result<Lock> {
        let root = store::evals_dir(dir);
        secrets::ensure_private_dir(&root)?;
        let path = root.join(".lock");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Lock(path)),
            Err(_) => Err(prereq(format!(
                "another eval is running (remove {} if it isn't)",
                path.display()
            ))),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Removes the eval project and the run's files, on every exit path.
struct Teardown<'a> {
    dir: &'a Path,
    compose: &'a dyn Compose,
}

impl Drop for Teardown<'_> {
    fn drop(&mut self) {
        let _ = self.compose.down(true);
        let _ = std::fs::remove_dir_all(run_dir(self.dir));
    }
}

fn result_line(r: &TaskResult) -> String {
    if r.passed {
        format!(
            "{:<16} PASS  {} calls  {} turns  {:.1}s",
            r.task, r.tool_calls, r.turns, r.seconds
        )
    } else {
        let mut s = format!(
            "{:<16} FAIL  {}",
            r.task,
            r.reasons.first().cloned().unwrap_or_default()
        );
        for more in r.reasons.iter().skip(1) {
            s.push_str(&format!("\n{:<22}{more}", ""));
        }
        s
    }
}

pub fn summary(run: &Run) -> String {
    format!(
        "{} ({} on {}), suite {}: {}/{} passed, threshold {} → {}",
        run.alias,
        run.model,
        run.server,
        run.suite_version,
        run.passed,
        run.total,
        run.threshold,
        if run.verdict { "PASS" } else { "FAIL" }
    )
}

struct Partial {
    run: Run,
    transcripts: Vec<(String, Value)>,
}

/// Runs the suite; returns the verdict (contracts/cli.md).
pub fn run(dir: &Path, opts: &Options) -> Result<bool> {
    if !dir.join(crate::config::FILE_NAME).exists() {
        return Err(prereq(
            "no sesh config; run `sesh init` and `sesh up` first",
        ));
    }
    let config = Config::load(dir)?;
    let secrets = Secrets::load(dir)?;
    let Some(key) = secrets.get(secrets::EVAL_ROUTER_KEY).map(str::to_string) else {
        return Err(prereq("no eval router key yet; run `sesh up` first"));
    };
    let alias = opts.alias.clone();
    let Some(model) = config.aliases.get(&alias).cloned() else {
        bail!("no alias {alias} in sesh.toml");
    };
    let server = strip_credentials(&config.inference.base_url);
    let label = format!("{alias} ({model} on {server})");
    let tasks = suite::suite();
    let max_secs = tasks
        .iter()
        .map(|t| limits(&t.limits, opts).seconds)
        .max()
        .unwrap_or(120);
    let http = UreqHttp::new(Duration::from_secs(max_secs));
    let router = crate::up::router_url(&config);
    match http.get(&format!("{router}/v1/models"), Some(&key)) {
        Ok(r) if r.status == 200 => {}
        _ => {
            return Err(prereq(format!(
                "the router at {router} doesn't answer; run `sesh up` first"
            )));
        }
    }

    let _lock = Lock::take(dir)?;
    let token = secrets::random_token()?;
    let prepared = prepare(dir, &config, &token)?;
    let audit = prepared.audit.clone();
    let compose = eval_compose_with(dir, &prepared.ntfy_gateway_pw, &prepared.admin_token);
    let approver = Approver::new(&prepared.ntfy_person_pw);
    let mut redact = secrets.values();
    redact.push(token.clone());
    redact.push(prepared.ntfy_gateway_pw.clone());
    redact.push(prepared.ntfy_person_pw.clone());
    redact.push(prepared.admin_token.clone());

    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let id = format!("{}-{alias}", store::utc_stamp(started));
    let partial = Arc::new(Mutex::new(Partial {
        run: Run {
            id: id.clone(),
            alias: alias.clone(),
            model,
            server,
            suite_version: SUITE_VERSION,
            sesh_version: env!("CARGO_PKG_VERSION").into(),
            threshold: opts.threshold.unwrap_or(tasks.len()).min(tasks.len()),
            results: vec![],
            passed: 0,
            total: tasks.len(),
            verdict: false,
            complete: false,
        },
        transcripts: vec![],
    }));
    {
        // Ctrl-C: remove the eval project, save what ran as incomplete, exit 130.
        let (p, d, r, n) = (
            partial.clone(),
            dir.to_path_buf(),
            redact.clone(),
            tasks.len(),
        );
        let _ = ctrlc::set_handler(move || {
            let _ = eval_compose(&d).down(true);
            let _ = std::fs::remove_dir_all(run_dir(&d));
            if let Ok(mut p) = p.lock() {
                p.run.complete = false;
                p.run.tally(n);
                let _ = store::save(&d, &p.run, &p.transcripts, &r);
            }
            let _ = std::fs::remove_file(store::evals_dir(&d).join(".lock"));
            eprintln!("interrupted; the eval project was removed and the run saved as incomplete");
            std::process::exit(130);
        });
    }

    let _teardown = Teardown {
        dir,
        compose: &compose,
    };
    compose
        .up(&[])
        .context("can't start the eval gateway (are the sesh images built?)")?;
    connect_fake_google(&http, &prepared.admin_token)?;
    let mcp = HttpMcp::connect(
        &format!("http://127.0.0.1:{EVAL_PORT}/mcp"),
        &token,
        Duration::from_secs(max_secs + 10),
    )?;
    let (mcp_tools, instructions) = mcp.list_tools()?;
    let tools = run_loop::openai_tools(&mcp_tools);
    let system = run_loop::system_prompt(&instructions);
    let chat = run_loop::RouterChat {
        http: &http,
        url: router,
        key,
        label,
    };

    let mut offset = 0;
    for task in &tasks {
        // The harness answers this task's approvals itself (research.md R8).
        let approver = &approver;
        let decide = task.approve.map(|d| move || approver.answer(d));
        let hooks = run_loop::Hooks {
            approver: decide.as_ref().map(|f| f as &dyn Fn() -> usize),
            followup: task.followup,
        };
        let outcome = run_loop::run_with(
            &chat,
            &mcp as &dyn Mcp,
            &alias,
            &tools,
            &system,
            &task.prompt,
            &limits(&task.limits, opts),
            &hooks,
        );
        // Decisions are audited by the eval gateway's page; let them land.
        if task.approve.is_some() {
            std::thread::sleep(Duration::from_millis(200));
        }
        let (calls, next) = read_audit(&audit, offset);
        offset = next;
        let result = score::score(task, &outcome, &calls);
        if !opts.json {
            println!("{}", result_line(&result));
        }
        let transcript = json!({
            "untrusted": true,
            "task": task.id,
            "messages": outcome.messages,
            "audit": calls,
        });
        let mut p = partial.lock().expect("lock");
        p.transcripts.push((task.id.to_string(), transcript));
        p.run.results.push(result);
    }

    let mut p = partial.lock().expect("lock");
    p.run.complete = true;
    p.run.tally(tasks.len());
    let saved = store::save(dir, &p.run, &p.transcripts, &redact)?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&p.run)?);
    } else {
        println!("{}", summary(&p.run));
        println!("saved {}", saved.display());
    }
    Ok(p.run.verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-eval-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn prepare_writes_only_the_token_hash_and_fake_upstreams() {
        let dir = tempdir("prep");
        let config = Config::new("http://srv:8080/v1", "qwen3");
        let token = "eval-token-value-123456";
        let audit = prepare(&dir, &config, token).unwrap().audit;
        let rdir = run_dir(&dir);
        let agents = std::fs::read_to_string(rdir.join("gateway/agents.toml")).unwrap();
        assert!(!agents.contains(token) && agents.contains(&token_hash(token)));
        let file = AgentsFile::parse(&agents).unwrap();
        assert_eq!(file.agents[0].name, EVAL_AGENT);
        assert_eq!(file.agents[0].connectors, EVAL_CONNECTORS.to_vec());
        let g: GatewaySettings =
            toml::from_str(&std::fs::read_to_string(rdir.join("gateway/gateway.toml")).unwrap())
                .unwrap();
        assert_eq!(g.upstreams.searxng, FAKES);
        assert_eq!(g.upstreams.open_meteo, FAKES);
        // specs/005: Google and Paperless are the fakes too; nothing real.
        assert_eq!(g.upstreams.paperless, FAKES);
        for u in [
            &g.upstreams.google_token,
            &g.upstreams.gmail,
            &g.upstreams.calendar,
            &g.upstreams.people,
            &g.upstreams.google_revoke,
        ] {
            assert!(u.starts_with(FAKES), "{u}");
        }
        assert!(g.home.is_some());
        assert_eq!(audit, rdir.join("audit/audit.jsonl"));
        assert!(rdir.join("compose.yaml").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                m(&store::evals_dir(&dir)),
                0o700,
                "the evals dir keeps other users out"
            );
            assert_eq!(m(&rdir.join("audit")), 0o777);
        }
    }

    #[test]
    fn reads_only_new_audit_lines() {
        let dir = tempdir("audit");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("audit.jsonl");
        std::fs::write(
            &p,
            "{\"outcome\":\"allowed\",\"connector\":\"search\",\"method\":\"web\"}\n",
        )
        .unwrap();
        let (a, off) = read_audit(&p, 0);
        assert_eq!(a.len(), 1);
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        use std::io::Write;
        writeln!(
            f,
            "{{\"outcome\":\"refused\",\"reason\":\"approval required\"}}"
        )
        .unwrap();
        let (b, _) = read_audit(&p, off);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].outcome, "refused");
        assert!(read_audit(&dir.join("missing"), 0).0.is_empty());
    }

    #[test]
    fn one_run_at_a_time() {
        let dir = tempdir("lock");
        let a = Lock::take(&dir).unwrap();
        let e = Lock::take(&dir).err().unwrap();
        assert!(e.downcast_ref::<Prerequisite>().is_some());
        drop(a);
        assert!(Lock::take(&dir).is_ok());
    }

    #[test]
    fn limit_flags_override_task_defaults() {
        let o = Options {
            turns: Some(3),
            timeout: Some(600),
            ..Default::default()
        };
        assert_eq!(
            limits(&Limits::default(), &o),
            Limits {
                turns: 3,
                tool_calls: 6,
                seconds: 600
            }
        );
    }

    #[test]
    fn server_labels_drop_credentials() {
        assert_eq!(
            strip_credentials("http://u:p@srv:8080/v1"),
            "http://srv:8080/v1"
        );
        assert_eq!(
            strip_credentials("http://srv:8080/v1"),
            "http://srv:8080/v1"
        );
    }

    #[test]
    fn needs_a_config_first() {
        let e = run(
            &tempdir("noconf"),
            &Options {
                alias: "agent-main".into(),
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(e.downcast_ref::<Prerequisite>().is_some(), "{e}");
    }

    #[test]
    fn result_lines() {
        let mut r = TaskResult {
            task: "web-fact".into(),
            passed: true,
            reasons: vec![],
            tool_calls: 2,
            turns: 3,
            seconds: 4.1,
            calls: vec![],
            answer: String::new(),
        };
        assert_eq!(
            result_line(&r),
            "web-fact         PASS  2 calls  3 turns  4.1s"
        );
        r.passed = false;
        r.reasons = vec!["expected a geo.forecast call that was allowed; none made".into()];
        assert!(result_line(&r).starts_with("web-fact         FAIL  expected a geo.forecast"));
    }
}
