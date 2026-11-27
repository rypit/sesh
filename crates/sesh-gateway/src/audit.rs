//! Seam: the only module that writes the audit log (data-model.md, Audit
//! entry). The gateway's database role can only INSERT; it can't read,
//! change or delete entries. If a write fails, the caller refuses the call.

use serde_json::{Value, json};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tokio_postgres::NoTls;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub agent: String,
    pub execution: Option<String>,
    pub connector: Option<String>,
    pub method: Option<String>,
    pub args_summary: Value,
    /// "allowed", "refused" or "failed".
    pub outcome: &'static str,
    pub reason: Option<String>,
}

pub enum Audit {
    Postgres {
        url: String,
        client: tokio::sync::Mutex<Option<tokio_postgres::Client>>,
    },
    /// The eval gateway only (specs/003-model-evals/contracts/gateway-audit-file.md):
    /// one JSON object per line, appended and flushed before the call returns.
    File {
        path: PathBuf,
        lock: tokio::sync::Mutex<()>,
    },
    /// For tests: records entries, or fails every write when `broken`.
    Memory {
        entries: Mutex<Vec<Entry>>,
        broken: bool,
    },
}

const INSERT: &str = "INSERT INTO audit (agent, execution, connector, method, args_summary, outcome, reason) \
                      VALUES ($1, $2, $3, $4, $5::text::jsonb, $6, $7)";

/// The only directory the file sink may write under.
pub const FILE_AUDIT_ROOT: &str = "/var/log/sesh-eval/";

impl Audit {
    /// A file sink, refused unless `path` is a plain path under `root`.
    pub fn file_under(path: &str, root: &str) -> anyhow::Result<Self> {
        let p = Path::new(path);
        let plain = p.is_absolute()
            && p.components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_)));
        anyhow::ensure!(
            plain && path.starts_with(root) && path.len() > root.len(),
            "the file audit sink must be a file under {root}"
        );
        Ok(Audit::File {
            path: p.to_path_buf(),
            lock: tokio::sync::Mutex::new(()),
        })
    }

    /// The file sink for `file:<path>` URLs, under [`FILE_AUDIT_ROOT`].
    pub fn file(path: &str) -> anyhow::Result<Self> {
        Self::file_under(path, FILE_AUDIT_ROOT)
    }

    pub fn postgres(url: String) -> Self {
        Audit::Postgres {
            url,
            client: tokio::sync::Mutex::new(None),
        }
    }

    pub fn memory() -> Self {
        Audit::Memory {
            entries: Mutex::default(),
            broken: false,
        }
    }

    pub async fn write(&self, e: &Entry) -> anyhow::Result<()> {
        match self {
            Audit::File { path, lock } => {
                let _held = lock.lock().await;
                let at_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let line = json!({
                    "at_ms": at_ms,
                    "agent": e.agent,
                    "execution": e.execution,
                    "connector": e.connector,
                    "method": e.method,
                    "args_summary": e.args_summary,
                    "outcome": e.outcome,
                    "reason": e.reason,
                });
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                writeln!(f, "{line}")?;
                f.sync_data()?;
                Ok(())
            }
            Audit::Memory { entries, broken } => {
                anyhow::ensure!(!broken, "audit store unavailable");
                entries.lock().expect("lock").push(e.clone());
                Ok(())
            }
            Audit::Postgres { url, client } => {
                let mut guard = client.lock().await;
                for attempt in 0..2 {
                    if guard.is_none() {
                        let (c, conn) = tokio_postgres::connect(url, NoTls).await?;
                        tokio::spawn(async move {
                            let _ = conn.await;
                        });
                        *guard = Some(c);
                    }
                    let c = guard.as_ref().expect("connected");
                    let summary = e.args_summary.to_string();
                    let r = c
                        .execute(
                            INSERT,
                            &[
                                &e.agent,
                                &e.execution,
                                &e.connector,
                                &e.method,
                                &summary,
                                &e.outcome,
                                &e.reason,
                            ],
                        )
                        .await;
                    match r {
                        Ok(_) => return Ok(()),
                        Err(err) if attempt == 0 && err.is_closed() => *guard = None,
                        Err(err) => return Err(err.into()),
                    }
                }
                anyhow::bail!("audit store unavailable")
            }
        }
    }

    #[cfg(test)]
    pub fn entries(&self) -> Vec<Entry> {
        match self {
            Audit::Memory { entries, .. } => entries.lock().expect("lock").clone(),
            Audit::Postgres { .. } | Audit::File { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;

    fn entry(outcome: &'static str) -> Entry {
        Entry {
            agent: "eval".into(),
            execution: Some("x1".into()),
            connector: Some("search".into()),
            method: Some("web".into()),
            args_summary: json!({"query": "varden"}),
            outcome,
            reason: None,
        }
    }

    #[test]
    fn refuses_paths_outside_the_root() {
        for bad in [
            "/etc/passwd",
            "/var/log/sesh-eval/../x",
            "var/log/sesh-eval/a",
            "/var/log/sesh-eval/",
            "/var/log/sesh-evalx/a",
        ] {
            assert!(Audit::file(bad).is_err(), "{bad} accepted");
        }
        assert!(Audit::file("/var/log/sesh-eval/audit.jsonl").is_ok());
    }

    #[tokio::test]
    async fn appends_one_json_line_per_entry() {
        let dir = std::env::temp_dir().join(format!("sesh-audit-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audit.jsonl");
        let _ = std::fs::remove_file(&path);
        let root = format!("{}/", dir.display());
        let a = Audit::file_under(path.to_str().unwrap(), &root).unwrap();
        a.write(&entry("allowed")).await.unwrap();
        a.write(&entry("refused")).await.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["outcome"], "allowed");
        assert_eq!(lines[1]["outcome"], "refused");
        assert_eq!(lines[1]["method"], "web");
    }

    #[tokio::test]
    async fn a_failed_write_is_an_error() {
        let a = Audit::file_under("/nonexistent-dir/x/audit.jsonl", "/nonexistent-dir/").unwrap();
        assert!(a.write(&entry("allowed")).await.is_err());
    }
}
