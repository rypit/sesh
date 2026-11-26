//! Per-execution capabilities (research.md R3, ADR 0004).
//!
//! A capability is 32 random bytes, valid for one agent and one execution
//! until it expires or is revoked. It is accepted only on the
//! executor-facing listener. The gateway is the only party that checks it,
//! so no signing key is needed.

use base64::Engine;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub agent: String,
    pub execution: String,
    pub expires_at: Instant,
    pub calls: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Unknown,
    Expired,
    TooManyCalls,
}

#[derive(Default)]
pub struct Capabilities {
    live: Mutex<HashMap<String, Grant>>,
}

fn random(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn new_execution_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random source");
    format!(
        "exec_{}",
        buf.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

impl Capabilities {
    /// Issues a capability for `agent`'s `execution`, valid for `ttl`.
    pub fn issue(&self, agent: &str, execution: &str, ttl: Duration) -> String {
        let cap = random(32);
        let mut live = self.live.lock().expect("lock");
        let now = Instant::now();
        live.retain(|_, g| g.expires_at > now);
        live.insert(
            cap.clone(),
            Grant {
                agent: agent.to_string(),
                execution: execution.to_string(),
                expires_at: now + ttl,
                calls: 0,
            },
        );
        cap
    }

    /// Checks a capability and counts a call against `max_calls`.
    pub fn use_for_call(&self, cap: &str, max_calls: u32) -> Result<Grant, Refusal> {
        let mut live = self.live.lock().expect("lock");
        let Some(g) = live.get_mut(cap) else {
            return Err(Refusal::Unknown);
        };
        if g.expires_at <= Instant::now() {
            live.remove(cap);
            return Err(Refusal::Expired);
        }
        if g.calls >= max_calls {
            return Err(Refusal::TooManyCalls);
        }
        g.calls += 1;
        Ok(g.clone())
    }

    pub fn revoke(&self, cap: &str) {
        self.live.lock().expect("lock").remove(cap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_capability_works_until_revoked() {
        let c = Capabilities::default();
        let cap = c.issue("alice", "exec_1", Duration::from_secs(30));
        let g = c.use_for_call(&cap, 10).unwrap();
        assert_eq!(
            (g.agent.as_str(), g.execution.as_str()),
            ("alice", "exec_1")
        );
        c.revoke(&cap);
        assert_eq!(c.use_for_call(&cap, 10), Err(Refusal::Unknown));
    }

    #[test]
    fn forged_and_expired_are_refused() {
        let c = Capabilities::default();
        assert_eq!(c.use_for_call("forged", 10), Err(Refusal::Unknown));
        let cap = c.issue("alice", "exec_1", Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(c.use_for_call(&cap, 10), Err(Refusal::Expired));
    }

    #[test]
    fn counts_calls() {
        let c = Capabilities::default();
        let cap = c.issue("alice", "exec_1", Duration::from_secs(30));
        assert!(c.use_for_call(&cap, 2).is_ok());
        assert!(c.use_for_call(&cap, 2).is_ok());
        assert_eq!(c.use_for_call(&cap, 2), Err(Refusal::TooManyCalls));
    }

    #[test]
    fn capabilities_are_unique_and_long() {
        let c = Capabilities::default();
        let a = c.issue("a", "e", Duration::from_secs(1));
        let b = c.issue("a", "e", Duration::from_secs(1));
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(new_execution_id().starts_with("exec_"));
    }
}
