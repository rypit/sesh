//! File references (specs/005 research.md R5, specs/010 research.md R9):
//! opaque ids the gateway issues to one agent for one file (a mail
//! attachment, a stored document or a browser download), so a script can
//! file or upload it without the bytes ever entering model context.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Where a referenced file's bytes come from.
#[derive(Clone, PartialEq)]
pub enum Source {
    /// A Gmail attachment, fetched from Google when used.
    Gmail {
        message_id: String,
        attachment_id: String,
    },
    /// One of the owner's Paperless documents, fetched when used.
    Document { id: u64 },
    /// A browser download, held by the gateway.
    Download { bytes: std::sync::Arc<Vec<u8>> },
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Gmail { message_id, .. } => write!(f, "Gmail({message_id})"),
            Source::Document { id } => write!(f, "Document({id})"),
            Source::Download { bytes } => write!(f, "Download({} bytes)", bytes.len()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub agent: String,
    pub owner: String,
    pub source: Source,
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

    /// How many live references `agent` holds of downloads.
    pub fn downloads_of(&self, agent: &str) -> Vec<(String, Attachment)> {
        let m = self.map.lock().expect("lock");
        let now = Instant::now();
        m.iter()
            .filter(|(_, (a, until))| {
                *until > now && a.agent == agent && matches!(a.source, Source::Download { .. })
            })
            .map(|(r, (a, _))| (r.clone(), a.clone()))
            .collect()
    }

    /// Forgets `r` (the oldest download, when an agent holds too many).
    pub fn drop_ref(&self, r: &str) {
        self.map.lock().expect("lock").remove(r);
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
            source: Source::Gmail {
                message_id: "m1".into(),
                attachment_id: "a1".into(),
            },
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
