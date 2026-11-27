//! Seam: the only module that reaches the approval channel (ntfy;
//! specs/004 research.md R3). The gateway writes every word; the link's
//! secret is in its URL fragment.

use anyhow::{Result, bail};
use serde_json::json;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// The owner's approval topic (specs/005 research.md R2).
    pub topic: String,
    pub title: String,
    pub message: String,
    /// The approval page, with the secret in the fragment.
    pub link: String,
}

pub enum Notifier {
    Ntfy {
        base: String,
        user: String,
        password: String,
        http: reqwest::Client,
    },
    /// For tests: records notes, or fails every send when `broken`.
    Memory {
        sent: Mutex<Vec<Note>>,
        broken: bool,
    },
}

impl Notifier {
    pub fn ntfy(base: &str, user: &str, password: &str) -> Result<Self> {
        Ok(Notifier::Ntfy {
            base: base.trim_end_matches('/').to_string(),
            user: user.to_string(),
            password: password.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    pub fn memory() -> Self {
        Notifier::Memory {
            sent: Mutex::default(),
            broken: false,
        }
    }

    pub fn broken() -> Self {
        Notifier::Memory {
            sent: Mutex::default(),
            broken: true,
        }
    }

    /// The ntfy publish body (contracts/approvals.md).
    pub fn body(topic: &str, n: &Note) -> serde_json::Value {
        json!({
            "topic": topic,
            "title": n.title,
            "message": n.message,
            "priority": 4,
            "actions": [{"action": "view", "label": "Review", "url": n.link, "clear": true}],
        })
    }

    pub async fn send(&self, n: &Note) -> Result<()> {
        match self {
            Notifier::Memory { sent, broken } => {
                if *broken {
                    bail!("approval channel unavailable");
                }
                sent.lock().expect("lock").push(n.clone());
                Ok(())
            }
            Notifier::Ntfy {
                base,
                user,
                password,
                http,
            } => {
                let topic = &n.topic;
                if topic.is_empty() {
                    bail!("no approval topic configured");
                }
                let r = http
                    .post(format!("{base}/"))
                    .basic_auth(user, Some(password))
                    .json(&Self::body(topic, n))
                    .send()
                    .await?;
                if !r.status().is_success() {
                    bail!("ntfy answered {}", r.status());
                }
                Ok(())
            }
        }
    }

    pub fn sent(&self) -> Vec<Note> {
        match self {
            Notifier::Memory { sent, .. } => sent.lock().expect("lock").clone(),
            Notifier::Ntfy { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_body_has_one_view_action() {
        let n = Note {
            topic: "t".into(),
            title: "Approve".into(),
            message: "m".into(),
            link: "http://x/approve/1#s".into(),
        };
        let b = Notifier::body("t", &n);
        assert_eq!(b["topic"], "t");
        assert_eq!(b["actions"].as_array().unwrap().len(), 1);
        assert_eq!(b["actions"][0]["action"], "view");
        assert_eq!(b["actions"][0]["url"], "http://x/approve/1#s");
    }

    #[tokio::test]
    async fn memory_records_and_broken_fails() {
        let m = Notifier::memory();
        let n = Note {
            topic: "t".into(),
            title: "t".into(),
            message: "m".into(),
            link: "l".into(),
        };
        m.send(&n).await.unwrap();
        assert_eq!(m.sent(), vec![n.clone()]);
        assert!(Notifier::broken().send(&n).await.is_err());
    }
}
