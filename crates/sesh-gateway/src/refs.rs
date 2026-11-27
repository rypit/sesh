//! Attachment references (specs/005 research.md R5): opaque ids the
//! gateway issues to one agent for one attachment, so a script can file it
//! without the bytes ever entering model context.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub agent: String,
    pub owner: String,
    pub message_id: String,
    pub attachment_id: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Default)]
pub struct Refs {
    map: Mutex<HashMap<String, (Attachment, Instant)>>,
}

impl Refs {
    pub fn issue(&self, a: Attachment, ttl: Duration) -> String {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).expect("OS random source");
        let r: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let mut m = self.map.lock().expect("lock");
        let now = Instant::now();
        m.retain(|_, (_, until)| *until > now);
        m.insert(r.clone(), (a, now + ttl));
        r
    }

    /// The attachment, if `r` is live and was issued to `agent`.
    pub fn resolve(&self, r: &str, agent: &str) -> Option<Attachment> {
        let m = self.map.lock().expect("lock");
        match m.get(r) {
            Some((a, until)) if *until > Instant::now() && a.agent == agent => Some(a.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(agent: &str) -> Attachment {
        Attachment {
            agent: agent.into(),
            owner: "ana".into(),
            message_id: "m1".into(),
            attachment_id: "a1".into(),
            filename: "invoice.pdf".into(),
            mime: "application/pdf".into(),
            size: 10,
        }
    }

    #[test]
    fn refs_belong_to_one_agent_and_expire() {
        let r = Refs::default();
        let id = r.issue(att("ana-agent"), Duration::from_secs(60));
        assert_eq!(id.len(), 32);
        assert_eq!(r.resolve(&id, "ana-agent").unwrap().filename, "invoice.pdf");
        assert!(r.resolve(&id, "ben-agent").is_none(), "another agent's ref");
        assert!(r.resolve("made-up", "ana-agent").is_none());
        let short = r.issue(att("ana-agent"), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        assert!(r.resolve(&short, "ana-agent").is_none(), "expired");
    }
}
