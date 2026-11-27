//! Runs workerd inside an executor container and enforces each execution's
//! deadline (research.md R2). Local workerd has no CPU limits, so a
//! runaway script is stopped by killing and restarting workerd.
//!
//! Untrusted side: it holds no secrets and the gateway trusts nothing it
//! reports. Environment:
//! - `SESH_GATEWAY_ADDRESS` (default `gateway:7001`): the executor's only route out
//! - `SESH_EXECUTOR_DIR` (default `/opt/sesh-executor`): holds `workerd.capnp.template` and `dist/`
//! - `SESH_EXECUTOR_DATA` (default `/var/lib/sesh-executor`): Durable Object storage
//! - `WORKERD` (default `/usr/local/bin/workerd`)
//! - `SUPERVISOR_LISTEN` (default `0.0.0.0:8000`)
//!
//! `sesh-executor-supervisor health` is the container's healthcheck: it
//! exits 0 only if the supervisor's `/health` answers 200, which needs
//! workerd running and answering its own `/health`.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::Read;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const WORKERD_ADDR: &str = "127.0.0.1:8080";
const GRACE: Duration = Duration::from_secs(1);

fn env(k: &str, default: &str) -> String {
    std::env::var(k)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Fills the config template; `gateway` must be `host:port` with no quotes.
fn render(template: &str, gateway: &str, data: &str, dist: &str) -> Result<String, String> {
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.:/".contains(c))
    };
    if !ok(gateway) || !ok(data) || !ok(dist) {
        return Err("invalid gateway address or path".into());
    }
    Ok(template
        .replace("@GATEWAY@", gateway)
        .replace("@DATA@", data)
        .replace("@DIST@", dist))
}

/// Deadline for one request: the body's `deadline_ms` (capped at 10 min) plus a grace.
fn deadline_of(body: &[u8]) -> Duration {
    let ms = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["deadline_ms"].as_u64())
        .unwrap_or(30_000)
        .min(600_000);
    Duration::from_millis(ms) + GRACE
}

/// workerd's command line. The config embeds `host.js` by absolute path,
/// and Cap'n Proto resolves absolute `embed`/`import` paths against the
/// import path rather than the filesystem, so `/` must be on it (without
/// it workerd exits at start with "Couldn't read file for embed").
fn workerd_args(config: &Path) -> Vec<OsString> {
    vec![
        "serve".into(),
        config.as_os_str().to_owned(),
        "--experimental".into(),
        "--import-path".into(),
        "/".into(),
    ]
}

/// True when `url` answers HTTP 200 within two seconds.
fn probe(url: &str) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build()
        .into();
    matches!(agent.get(url).call(), Ok(r) if r.status() == 200)
}

/// The executor is healthy only if workerd's process runs and it serves.
fn healthy(process_alive: bool, answers: bool) -> bool {
    process_alive && answers
}

struct Workerd {
    bin: String,
    config: PathBuf,
    child: Option<Child>,
}

impl Workerd {
    fn start(&mut self) {
        let child = Command::new(&self.bin)
            .args(workerd_args(&self.config))
            .spawn()
            .expect("start workerd");
        self.child = Some(child);
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            if TcpStream::connect(WORKERD_ADDR).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("workerd did not start listening in time");
    }

    fn restart(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.start();
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("health") {
        let listen = env("SUPERVISOR_LISTEN", "0.0.0.0:8000");
        let port = listen.rsplit(':').next().unwrap_or("8000");
        std::process::exit(if probe(&format!("http://127.0.0.1:{port}/health")) {
            0
        } else {
            1
        });
    }
    let dir = PathBuf::from(env("SESH_EXECUTOR_DIR", "/opt/sesh-executor"));
    let data = env("SESH_EXECUTOR_DATA", "/var/lib/sesh-executor");
    std::fs::create_dir_all(&data).expect("create data dir");
    let template =
        std::fs::read_to_string(dir.join("workerd.capnp.template")).expect("read config template");
    let dist = dir.join("dist").display().to_string();
    let config = render(
        &template,
        &env("SESH_GATEWAY_ADDRESS", "gateway:7001"),
        &data,
        &dist,
    )
    .expect("render config");
    // Written to the data dir so the image itself can stay read-only.
    let config_path = PathBuf::from(&data).join("workerd.capnp");
    std::fs::write(&config_path, config).expect("write config");

    let mut workerd = Workerd {
        bin: env("WORKERD", "/usr/local/bin/workerd"),
        config: config_path,
        child: None,
    };
    workerd.start();

    let server =
        tiny_http::Server::http(env("SUPERVISOR_LISTEN", "0.0.0.0:8000")).expect("bind supervisor");
    // One request at a time: an agent runs one script at a time.
    for mut req in server.incoming_requests() {
        // Restart after replying, so the timeout reaches the gateway at
        // once (SC-004) instead of after workerd's start-up.
        let mut restart_after = false;
        let path = req.url().split('?').next().unwrap_or("").to_string();
        let response = match (req.method().as_str(), path.as_str()) {
            ("GET", "/health") => {
                let answers = probe(&format!("http://{WORKERD_ADDR}/health"));
                if healthy(workerd.alive(), answers) {
                    tiny_http::Response::from_string("ok")
                } else {
                    workerd.restart();
                    tiny_http::Response::from_string("restarted").with_status_code(503)
                }
            }
            ("POST", "/execute") => {
                let mut body = Vec::new();
                let _ = req.as_reader().take(4 << 20).read_to_end(&mut body);
                let deadline = deadline_of(&body);
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(deadline))
                    .http_status_as_error(false)
                    .build()
                    .into();
                let r = agent
                    .post(&format!("http://{WORKERD_ADDR}/execute"))
                    .header("content-type", "application/json")
                    .send(&body[..]);
                match r {
                    Ok(mut resp) => {
                        let status = resp.status().as_u16();
                        let text = resp.body_mut().read_to_string().unwrap_or_default();
                        tiny_http::Response::from_string(text)
                            .with_status_code(status)
                            .with_header(
                                tiny_http::Header::from_bytes(
                                    &b"Content-Type"[..],
                                    &b"application/json"[..],
                                )
                                .expect("header"),
                            )
                    }
                    Err(_) => {
                        // Deadline passed (or workerd died): stop the runaway script.
                        if let Some(c) = workerd.child.as_mut() {
                            let _ = c.kill();
                        }
                        restart_after = true;
                        tiny_http::Response::from_string(r#"{"status":"timeout"}"#)
                            .with_status_code(504)
                    }
                }
            }
            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };
        let _ = req.respond(response);
        if restart_after {
            workerd.restart();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serves one fixed status on a random port; returns its /health URL.
    fn serve(status: u16) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/health", server.server_addr().to_ip().unwrap());
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = req.respond(tiny_http::Response::from_string("x").with_status_code(status));
            }
        });
        url
    }

    #[test]
    fn probe_needs_a_200_answer() {
        assert!(probe(&serve(200)));
        assert!(!probe(&serve(503)), "a 503 is unhealthy");
        // A port that accepts nothing: the old TCP-only check missed this case.
        assert!(!probe("http://127.0.0.1:9/health"));
    }

    #[test]
    fn healthy_only_when_workerd_runs_and_answers() {
        assert!(healthy(true, true));
        assert!(!healthy(true, false), "process up but not serving");
        assert!(!healthy(false, true));
    }

    #[test]
    fn renders_only_safe_values() {
        let t = "address = \"@GATEWAY@\", path = \"@DATA@\", embed \"@DIST@/host.js\"";
        assert_eq!(
            render(t, "gateway:7001", "/var/lib/x", "/opt/e/dist").unwrap(),
            "address = \"gateway:7001\", path = \"/var/lib/x\", embed \"/opt/e/dist/host.js\""
        );
        assert!(render(t, "gw\"), (x", "/d", "/e").is_err());
    }

    #[test]
    fn workerd_can_resolve_the_absolute_embed_path() {
        // Cap'n Proto looks up an absolute `embed` path (the rendered
        // `@DIST@/host.js`) in the import path, not the filesystem root.
        let args = workerd_args(Path::new("/var/lib/x/workerd.capnp"));
        let args: Vec<_> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args[..2], ["serve", "/var/lib/x/workerd.capnp"]);
        assert!(
            args.windows(2).any(|w| w == ["--import-path", "/"]),
            "{args:?}"
        );
        assert!(args.contains(&"--experimental"));
    }

    #[test]
    fn deadline_comes_from_the_body_with_a_cap() {
        assert_eq!(
            deadline_of(br#"{"deadline_ms": 2000}"#),
            Duration::from_millis(3000)
        );
        assert_eq!(deadline_of(b"not json"), Duration::from_millis(31_000));
        assert_eq!(
            deadline_of(br#"{"deadline_ms": 99999999}"#),
            Duration::from_millis(601_000)
        );
    }
}
