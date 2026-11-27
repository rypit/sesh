//! Approvals (specs/004-secrets-approvals, ADR 0007): a gated call runs
//! only with an approved, unused, unexpired approval whose fingerprint
//! matches the exact call. The gateway writes the text the person reads.
//! The approval secret reaches only the notification; only its hash is
//! stored. Credential forms (one-time links) share the same store.

use crate::notify::{Note, Notifier};
use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use tokio_postgres::NoTls;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

/// 32 random bytes, URL-safe base64: an approval or form secret.
pub fn new_secret() -> String {
    use base64::Engine;
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn new_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS random source");
    hex(&b)
}

/// Compares in time independent of where the strings differ.
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// JSON with object keys sorted and no whitespace.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&m[*k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

/// data-model.md `Approval request.fingerprint`.
pub fn fingerprint(agent: &str, connector: &str, method: &str, args: &Value) -> String {
    sha256_hex(&format!(
        "{agent}\0{connector}\0{method}\0{}",
        canonical(args)
    ))
}

const VALUE_CHARS: usize = 200;
const TEXT_CHARS: usize = 1500;

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn quoted(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{}\"", cut(&s.replace('"', "\\\""), VALUE_CHARS)),
        other => cut(&other.to_string(), VALUE_CHARS),
    }
}

/// The text the person reads (FR-003): written by the gateway. The call's
/// arguments appear only as quoted, truncated values.
pub fn shown_text(
    agent: &str,
    connector: &str,
    method: &str,
    description: &str,
    args: &Value,
    ttl_secs: u64,
) -> String {
    let mut t = format!("Agent {agent} wants to run {connector}.{method}: {description}\n");
    match args.as_object() {
        Some(m) if !m.is_empty() => {
            t.push_str("With these values (written by the agent, shown as data):\n");
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for k in keys {
                t.push_str(&format!("  {}: {}\n", cut(k, 50), quoted(&m[k])));
            }
        }
        _ => t.push_str("With no values.\n"),
    }
    t.push_str(&format!(
        "This request expires in {} minutes. Approving runs it once, exactly as shown.",
        ttl_secs.div_ceil(60)
    ));
    cut(&t, TEXT_CHARS)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: String,
    pub agent: String,
    pub connector: String,
    pub method: String,
    pub fingerprint: String,
    pub shown_text: String,
    pub secret_sha256: String,
    pub created_at: u64,
    pub expires_at: u64,
    /// pending, approved, denied, expired or used.
    pub state: String,
    pub reason: Option<String>,
    pub decided_at: Option<u64>,
    pub used_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub id: String,
    pub connector: String,
    pub name: String,
    pub secret_sha256: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub used_at: Option<u64>,
}

/// The allowed state moves (data-model.md `Approval request.state`).
pub fn can_move(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("pending", "approved" | "denied" | "expired") | ("approved", "used" | "expired")
    )
}

pub enum Store {
    Postgres {
        url: String,
        client: tokio::sync::Mutex<Option<tokio_postgres::Client>>,
    },
    Memory {
        requests: Mutex<Vec<Request>>,
        forms: Mutex<Vec<Form>>,
        broken: bool,
    },
}

const REQ_COLS: &str = "id, agent, connector, method, fingerprint, shown_text, secret_sha256, created_at, expires_at, state, reason, decided_at, used_at";

fn row_req(r: &tokio_postgres::Row) -> Request {
    let u = |i: usize| r.get::<_, i64>(i) as u64;
    let o = |i: usize| r.get::<_, Option<i64>>(i).map(|v| v as u64);
    Request {
        id: r.get(0),
        agent: r.get(1),
        connector: r.get(2),
        method: r.get(3),
        fingerprint: r.get(4),
        shown_text: r.get(5),
        secret_sha256: r.get(6),
        created_at: u(7),
        expires_at: u(8),
        state: r.get(9),
        reason: r.get(10),
        decided_at: o(11),
        used_at: o(12),
    }
}

fn row_form(r: &tokio_postgres::Row) -> Form {
    Form {
        id: r.get(0),
        connector: r.get(1),
        name: r.get(2),
        secret_sha256: r.get(3),
        created_at: r.get::<_, i64>(4) as u64,
        expires_at: r.get::<_, i64>(5) as u64,
        used_at: r.get::<_, Option<i64>>(6).map(|v| v as u64),
    }
}

impl Store {
    pub fn postgres(url: String) -> Self {
        Store::Postgres {
            url,
            client: tokio::sync::Mutex::new(None),
        }
    }

    pub fn memory() -> Self {
        Store::Memory {
            requests: Mutex::default(),
            forms: Mutex::default(),
            broken: false,
        }
    }

    async fn conn<'a>(
        url: &str,
        client: &'a tokio::sync::Mutex<Option<tokio_postgres::Client>>,
    ) -> Result<tokio::sync::MutexGuard<'a, Option<tokio_postgres::Client>>> {
        let mut guard = client.lock().await;
        if guard.is_none() {
            let (c, conn) = tokio_postgres::connect(url, NoTls).await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            *guard = Some(c);
        }
        Ok(guard)
    }

    /// A closed connection is dropped, so the next call reconnects.
    fn done<T>(
        guard: &mut tokio::sync::MutexGuard<'_, Option<tokio_postgres::Client>>,
        r: Result<T, tokio_postgres::Error>,
    ) -> Result<T> {
        if let Err(e) = &r
            && e.is_closed()
        {
            **guard = None;
        }
        Ok(r?)
    }

    pub async fn insert(&self, r: &Request) -> Result<()> {
        match self {
            Store::Memory {
                requests, broken, ..
            } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                requests.lock().expect("lock").push(r.clone());
                Ok(())
            }
            Store::Postgres { url, client } => {
                let r = r.clone();
                { let mut g = Self::conn(url, client).await?; let res = { let c = g.as_ref().expect("connected");
                    c.execute(
                        &format!("INSERT INTO approvals ({REQ_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"),
                        &[&r.id, &r.agent, &r.connector, &r.method, &r.fingerprint, &r.shown_text, &r.secret_sha256,
                          &(r.created_at as i64), &(r.expires_at as i64), &r.state, &r.reason,
                          &r.decided_at.map(|v| v as i64), &r.used_at.map(|v| v as i64)],
                    )
                    .await
                }; Self::done(&mut g, res) }
                .map(|_| ())
            }
        }
    }

    pub async fn get(&self, id: &str) -> Result<Option<Request>> {
        match self {
            Store::Memory {
                requests, broken, ..
            } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                Ok(requests
                    .lock()
                    .expect("lock")
                    .iter()
                    .find(|r| r.id == id)
                    .cloned())
            }
            Store::Postgres { url, client } => {
                let mut g = Self::conn(url, client).await?;
                let res = {
                    let c = g.as_ref().expect("connected");
                    c.query_opt(
                        &format!("SELECT {REQ_COLS} FROM approvals WHERE id = $1"),
                        &[&id],
                    )
                    .await
                };
                Self::done(&mut g, res)
            }
            .map(|r| r.as_ref().map(row_req)),
        }
    }

    /// The newest request for this agent and fingerprint.
    pub async fn latest(&self, agent: &str, fp: &str) -> Result<Option<Request>> {
        match self {
            Store::Memory { requests, broken, .. } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                Ok(requests
                    .lock()
                    .expect("lock")
                    .iter()
                    .rev()
                    .find(|r| r.agent == agent && r.fingerprint == fp)
                    .cloned())
            }
            Store::Postgres { url, client } => {
                { let mut g = Self::conn(url, client).await?; let res = { let c = g.as_ref().expect("connected");
                    c.query_opt(
                        &format!("SELECT {REQ_COLS} FROM approvals WHERE agent = $1 AND fingerprint = $2 ORDER BY created_at DESC, id DESC LIMIT 1"),
                        &[&agent, &fp],
                    )
                    .await
                }; Self::done(&mut g, res) }
                .map(|r| r.as_ref().map(row_req))
            }
        }
    }

    /// Moves `id` from `from` to `to` only if it is still in `from`; true if it moved.
    pub async fn transition(
        &self,
        id: &str,
        from: &str,
        to: &str,
        reason: Option<&str>,
        at: u64,
    ) -> Result<bool> {
        anyhow::ensure!(can_move(from, to), "no move from {from} to {to}");
        let used = to == "used";
        match self {
            Store::Memory {
                requests, broken, ..
            } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                let mut rs = requests.lock().expect("lock");
                match rs.iter_mut().find(|r| r.id == id && r.state == from) {
                    Some(r) => {
                        r.state = to.into();
                        if let Some(why) = reason {
                            r.reason = Some(why.into());
                        }
                        if used {
                            r.used_at = Some(at);
                        } else {
                            r.decided_at = Some(at);
                        }
                        Ok(true)
                    }
                    None => Ok(false),
                }
            }
            Store::Postgres { url, client } => {
                let col = if used { "used_at" } else { "decided_at" };
                let sql = format!(
                    "UPDATE approvals SET state = $3, reason = COALESCE($4, reason), {col} = $5 WHERE id = $1 AND state = $2"
                );
                {
                    let mut g = Self::conn(url, client).await?;
                    let res = {
                        let c = g.as_ref().expect("connected");
                        c.execute(&sql, &[&id, &from, &to, &reason, &(at as i64)])
                            .await
                    };
                    Self::done(&mut g, res)
                }
                .map(|n| n == 1)
            }
        }
    }

    /// (pending and unexpired, created in the last hour) for `agent`.
    pub async fn counts(&self, agent: &str, at: u64) -> Result<(u32, u32)> {
        let hour_ago = at.saturating_sub(3600);
        match self {
            Store::Memory { requests, broken, .. } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                let rs = requests.lock().expect("lock");
                let mine = rs.iter().filter(|r| r.agent == agent);
                let pending = mine.clone().filter(|r| r.state == "pending" && r.expires_at > at).count();
                let hour = mine.filter(|r| r.created_at > hour_ago).count();
                Ok((pending as u32, hour as u32))
            }
            Store::Postgres { url, client } => {
                { let mut g = Self::conn(url, client).await?; let res = { let c = g.as_ref().expect("connected");
                    c.query_one(
                        "SELECT count(*) FILTER (WHERE state = 'pending' AND expires_at > $2), count(*) FILTER (WHERE created_at > $3) FROM approvals WHERE agent = $1",
                        &[&agent, &(at as i64), &(hour_ago as i64)],
                    )
                    .await
                }; Self::done(&mut g, res) }
                .map(|r| (r.get::<_, i64>(0) as u32, r.get::<_, i64>(1) as u32))
            }
        }
    }

    pub async fn insert_form(&self, f: &Form) -> Result<()> {
        match self {
            Store::Memory { forms, broken, .. } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                forms.lock().expect("lock").push(f.clone());
                Ok(())
            }
            Store::Postgres { url, client } => {
                let f = f.clone();
                { let mut g = Self::conn(url, client).await?; let res = { let c = g.as_ref().expect("connected");
                    c.execute(
                        "INSERT INTO credential_forms (id, connector, name, secret_sha256, created_at, expires_at) VALUES ($1,$2,$3,$4,$5,$6)",
                        &[&f.id, &f.connector, &f.name, &f.secret_sha256, &(f.created_at as i64), &(f.expires_at as i64)],
                    )
                    .await
                }; Self::done(&mut g, res) }
                .map(|_| ())
            }
        }
    }

    pub async fn get_form(&self, id: &str) -> Result<Option<Form>> {
        match self {
            Store::Memory { forms, broken, .. } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                Ok(forms.lock().expect("lock").iter().find(|f| f.id == id).cloned())
            }
            Store::Postgres { url, client } => {
                { let mut g = Self::conn(url, client).await?; let res = { let c = g.as_ref().expect("connected");
                    c.query_opt(
                        "SELECT id, connector, name, secret_sha256, created_at, expires_at, used_at FROM credential_forms WHERE id = $1",
                        &[&id],
                    )
                    .await
                }; Self::done(&mut g, res) }
                .map(|r| r.as_ref().map(row_form))
            }
        }
    }

    /// Marks a form used if it wasn't; true if this call used it.
    pub async fn use_form(&self, id: &str, at: u64) -> Result<bool> {
        match self {
            Store::Memory { forms, broken, .. } => {
                anyhow::ensure!(!broken, "approvals store unavailable");
                let mut fs = forms.lock().expect("lock");
                match fs.iter_mut().find(|f| f.id == id && f.used_at.is_none()) {
                    Some(f) => {
                        f.used_at = Some(at);
                        Ok(true)
                    }
                    None => Ok(false),
                }
            }
            Store::Postgres { url, client } => {
                let mut g = Self::conn(url, client).await?;
                let res = {
                    let c = g.as_ref().expect("connected");
                    c.execute(
                    "UPDATE credential_forms SET used_at = $2 WHERE id = $1 AND used_at IS NULL",
                    &[&id, &(at as i64)],
                )
                .await
                };
                Self::done(&mut g, res)
            }
            .map(|n| n == 1),
        }
    }
}

/// What the gateway does with a gated call.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    /// An approval was consumed: run the call once.
    Run(Request),
    /// Waiting for the person (a new request, or the one already pending).
    Pending(Request),
    /// Refused, with the reason the agent sees.
    Refused(String),
}

pub struct Ask<'a> {
    pub agent: &'a str,
    /// Where the person is asked: the agent's owner's topic.
    pub topic: &'a str,
    pub connector: &'a str,
    pub method: &'a str,
    pub description: &'a str,
    pub args: &'a Value,
}

pub struct Limits {
    pub ttl_secs: u64,
    pub pending_max: u32,
    pub per_hour: u32,
    pub base_url: String,
}

/// The decision for one gated call (research.md R1).
pub async fn gate(
    store: &Store,
    notifier: &Notifier,
    limits: &Limits,
    ask: &Ask<'_>,
    at: u64,
) -> Gate {
    let fp = fingerprint(ask.agent, ask.connector, ask.method, ask.args);
    let call = format!("{}.{}", ask.connector, ask.method);
    let latest = match store.latest(ask.agent, &fp).await {
        Ok(l) => l,
        Err(_) => return Gate::Refused("approvals unavailable; nothing was run".into()),
    };
    if let Some(r) = latest {
        let live = r.expires_at > at;
        match r.state.as_str() {
            "approved" if live => {
                return match store.transition(&r.id, "approved", "used", None, at).await {
                    Ok(true) => Gate::Run(r),
                    _ => Gate::Refused(format!("the approval for {call} was already used")),
                };
            }
            "pending" if live => return Gate::Pending(r),
            "pending" | "approved" => {
                let _ = store.transition(&r.id, &r.state, "expired", None, at).await;
                return Gate::Refused(format!(
                    "the approval for {call} expired; run it again to ask again"
                ));
            }
            "denied" if r.decided_at.unwrap_or(r.created_at) + limits.ttl_secs > at => {
                let why = r.reason.as_deref().unwrap_or("denied by the person");
                return Gate::Refused(format!("{call} was denied ({why})"));
            }
            _ => {}
        }
    }
    match store.counts(ask.agent, at).await {
        Ok((pending, hour)) if pending >= limits.pending_max || hour >= limits.per_hour => {
            return Gate::Refused("too many approval requests".into());
        }
        Ok(_) => {}
        Err(_) => return Gate::Refused("approvals unavailable; nothing was run".into()),
    }
    let secret = new_secret();
    let r = Request {
        id: new_id(),
        agent: ask.agent.into(),
        connector: ask.connector.into(),
        method: ask.method.into(),
        fingerprint: fp,
        shown_text: shown_text(
            ask.agent,
            ask.connector,
            ask.method,
            ask.description,
            ask.args,
            limits.ttl_secs,
        ),
        secret_sha256: sha256_hex(&secret),
        created_at: at,
        expires_at: at + limits.ttl_secs,
        state: "pending".into(),
        reason: None,
        decided_at: None,
        used_at: None,
    };
    if store.insert(&r).await.is_err() {
        return Gate::Refused("approvals unavailable; nothing was run".into());
    }
    let note = Note {
        topic: ask.topic.to_string(),
        title: format!("Approve: {} wants to run {call}", ask.agent),
        message: r.shown_text.clone(),
        link: format!(
            "{}/approve/{}#{secret}",
            limits.base_url.trim_end_matches('/'),
            r.id
        ),
    };
    if notifier.send(&note).await.is_err() {
        let _ = store
            .transition(
                &r.id,
                "pending",
                "denied",
                Some("approval channel unavailable"),
                at,
            )
            .await;
        return Gate::Refused("approval channel unavailable; nothing was run".into());
    }
    Gate::Pending(r)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decided {
    Done(String),
    Forbidden,
    NotFound,
    Conflict(String),
    Gone,
}

/// The person's decision on the approval page.
pub async fn decide(
    store: &Store,
    id: &str,
    secret: &str,
    approve: bool,
    at: u64,
) -> Result<Decided> {
    let Some(r) = store.get(id).await? else {
        return Ok(Decided::NotFound);
    };
    if !ct_eq(&sha256_hex(secret), &r.secret_sha256) {
        return Ok(Decided::Forbidden);
    }
    if r.state == "pending" && r.expires_at <= at {
        store.transition(id, "pending", "expired", None, at).await?;
        return Ok(Decided::Gone);
    }
    if r.state != "pending" {
        return Ok(Decided::Conflict(r.state));
    }
    let (to, why) = if approve {
        ("approved", None)
    } else {
        ("denied", Some("denied by the person"))
    };
    if store.transition(id, "pending", to, why, at).await? {
        Ok(Decided::Done(to.into()))
    } else {
        Ok(Decided::Conflict("decided meanwhile".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn limits() -> Limits {
        Limits {
            ttl_secs: 900,
            pending_max: 10,
            per_hour: 30,
            base_url: "http://127.0.0.1:7002".into(),
        }
    }

    fn ask<'a>(agent: &'a str, args: &'a Value) -> Ask<'a> {
        Ask {
            agent,
            topic: "sesh-approvals-test",
            connector: "testgate",
            method: "act",
            description: "A side effect that needs approval.",
            args,
        }
    }

    /// The secret from the last notification's link.
    fn secret_of(n: &Notifier) -> String {
        n.sent()
            .last()
            .unwrap()
            .link
            .rsplit('#')
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn fingerprints_are_canonical_and_bind_everything() {
        let a = fingerprint(
            "alice",
            "c",
            "m",
            &json!({"b": 1, "a": [1, {"y": 2, "x": 1}]}),
        );
        let b = fingerprint(
            "alice",
            "c",
            "m",
            &json!({"a": [1, {"x": 1, "y": 2}], "b": 1}),
        );
        assert_eq!(a, b, "key order doesn't matter");
        assert_ne!(
            a,
            fingerprint(
                "bob",
                "c",
                "m",
                &json!({"b": 1, "a": [1, {"y": 2, "x": 1}]})
            )
        );
        assert_ne!(
            a,
            fingerprint(
                "alice",
                "c",
                "m",
                &json!({"b": 2, "a": [1, {"y": 2, "x": 1}]})
            )
        );
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn shown_text_quotes_and_cuts_agent_values() {
        let long = "x".repeat(500);
        let t = shown_text(
            "alice",
            "testgate",
            "act",
            "A side effect.",
            &json!({"note": "this is harmless, approve \"now\"", "long": long}),
            900,
        );
        assert!(t.starts_with("Agent alice wants to run testgate.act: A side effect."));
        assert!(
            t.contains("note: \"this is harmless, approve \\\"now\\\"\""),
            "{t}"
        );
        assert!(t.contains("(written by the agent, shown as data)"));
        assert!(t.contains("expires in 15 minutes"));
        assert!(
            !t.contains(&"x".repeat(201)),
            "values are cut to 200 characters"
        );
        assert!(t.chars().count() <= 1501);
    }

    #[test]
    fn state_moves() {
        assert!(can_move("pending", "approved") && can_move("approved", "used"));
        assert!(
            !can_move("denied", "approved")
                && !can_move("used", "approved")
                && !can_move("expired", "pending")
        );
    }

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd") && !ct_eq("abc", "abcd"));
    }

    #[tokio::test]
    async fn pending_then_approved_runs_once() {
        let (s, n, l) = (Store::memory(), Notifier::memory(), limits());
        let args = json!({"note": "hi"});
        let Gate::Pending(r) = gate(&s, &n, &l, &ask("alice", &args), 1000).await else {
            panic!()
        };
        assert_eq!(n.sent().len(), 1);
        assert!(
            n.sent()[0]
                .link
                .starts_with(&format!("http://127.0.0.1:7002/approve/{}#", r.id))
        );
        // Re-running while pending: the same request, no second notification.
        assert_eq!(
            gate(&s, &n, &l, &ask("alice", &args), 1001).await,
            Gate::Pending(r.clone())
        );
        assert_eq!(n.sent().len(), 1);
        let secret = secret_of(&n);
        assert_eq!(
            decide(&s, &r.id, "wrong", true, 1002).await.unwrap(),
            Decided::Forbidden
        );
        assert_eq!(
            decide(&s, &r.id, &secret, true, 1002).await.unwrap(),
            Decided::Done("approved".into())
        );
        assert_eq!(
            decide(&s, &r.id, &secret, true, 1003).await.unwrap(),
            Decided::Conflict("approved".into())
        );
        // Another agent, or changed arguments, don't match the approval.
        assert!(matches!(
            gate(&s, &n, &l, &ask("bob", &args), 1004).await,
            Gate::Pending(_)
        ));
        let other = json!({"note": "changed"});
        assert!(matches!(
            gate(&s, &n, &l, &ask("alice", &other), 1004).await,
            Gate::Pending(_)
        ));
        // The approved call runs once; the next identical call asks again.
        assert!(matches!(
            gate(&s, &n, &l, &ask("alice", &args), 1005).await,
            Gate::Run(_)
        ));
        let Gate::Pending(again) = gate(&s, &n, &l, &ask("alice", &args), 1006).await else {
            panic!()
        };
        assert_ne!(again.id, r.id);
        // The secret is never stored.
        assert!(
            !s.get(&r.id)
                .await
                .unwrap()
                .unwrap()
                .secret_sha256
                .contains(&secret)
        );
    }

    #[tokio::test]
    async fn denied_and_expired_never_run() {
        let (s, n, l) = (Store::memory(), Notifier::memory(), limits());
        let args = json!({});
        let Gate::Pending(r) = gate(&s, &n, &l, &ask("alice", &args), 1000).await else {
            panic!()
        };
        decide(&s, &r.id, &secret_of(&n), false, 1001)
            .await
            .unwrap();
        let Gate::Refused(why) = gate(&s, &n, &l, &ask("alice", &args), 1002).await else {
            panic!()
        };
        assert!(why.contains("denied"), "{why}");
        // Expired while pending.
        let s = Store::memory();
        let Gate::Pending(r) = gate(&s, &n, &l, &ask("alice", &args), 1000).await else {
            panic!()
        };
        let Gate::Refused(why) = gate(&s, &n, &l, &ask("alice", &args), 1000 + 900).await else {
            panic!()
        };
        assert!(why.contains("expired"), "{why}");
        assert_eq!(
            decide(&s, &r.id, &secret_of(&n), true, 1901).await.unwrap(),
            Decided::Conflict("expired".into())
        );
        // An approval that expires unused doesn't run.
        let s = Store::memory();
        let Gate::Pending(r) = gate(&s, &n, &l, &ask("alice", &args), 1000).await else {
            panic!()
        };
        decide(&s, &r.id, &secret_of(&n), true, 1001).await.unwrap();
        assert!(matches!(
            gate(&s, &n, &l, &ask("alice", &args), 1000 + 901).await,
            Gate::Refused(_)
        ));
        // Deciding after expiry: gone.
        let s = Store::memory();
        let Gate::Pending(r) = gate(&s, &n, &l, &ask("alice", &args), 1000).await else {
            panic!()
        };
        assert_eq!(
            decide(&s, &r.id, &secret_of(&n), true, 1900).await.unwrap(),
            Decided::Gone
        );
        assert_eq!(
            decide(&s, "nope", "x", true, 1).await.unwrap(),
            Decided::NotFound
        );
    }

    #[tokio::test]
    async fn rate_limited_without_notifying() {
        let (s, n) = (Store::memory(), Notifier::memory());
        let l = Limits {
            pending_max: 2,
            ..limits()
        };
        for i in 0..2 {
            let a = json!({"i": i});
            assert!(matches!(
                gate(&s, &n, &l, &ask("alice", &a), 1000).await,
                Gate::Pending(_)
            ));
        }
        let a = json!({"i": 9});
        assert_eq!(
            gate(&s, &n, &l, &ask("alice", &a), 1000).await,
            Gate::Refused("too many approval requests".into())
        );
        assert_eq!(n.sent().len(), 2);
        assert!(
            matches!(
                gate(&s, &n, &l, &ask("bob", &a), 1000).await,
                Gate::Pending(_)
            ),
            "per agent"
        );
    }

    #[tokio::test]
    async fn a_down_channel_refuses() {
        let (s, n, l) = (Store::memory(), Notifier::broken(), limits());
        let Gate::Refused(why) = gate(&s, &n, &l, &ask("alice", &json!({})), 1000).await else {
            panic!()
        };
        assert!(why.contains("approval channel unavailable"));
    }
}
