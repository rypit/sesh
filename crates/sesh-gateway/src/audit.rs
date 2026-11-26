//! Seam: the only module that writes the audit log (data-model.md, Audit
//! entry). The gateway's database role can only INSERT; it can't read,
//! change or delete entries. If a write fails, the caller refuses the call.

use serde_json::Value;
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
    /// For tests: records entries, or fails every write when `broken`.
    Memory {
        entries: Mutex<Vec<Entry>>,
        broken: bool,
    },
}

const INSERT: &str = "INSERT INTO audit (agent, execution, connector, method, args_summary, outcome, reason) \
                      VALUES ($1, $2, $3, $4, $5::text::jsonb, $6, $7)";

impl Audit {
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
            Audit::Postgres { .. } => Vec::new(),
        }
    }
}
