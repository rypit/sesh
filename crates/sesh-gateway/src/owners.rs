//! Owner scoping (specs/005 research.md R1, ADR 0008): everything a call
//! may touch for a person is derived here from the calling agent's owner,
//! never from anything the model passes.

use crate::Gateway;
use sesh_core::{Agent, HOUSEHOLD};

#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub owner: String,
    /// The owner's approval topic (the Household's is read by every user).
    pub topic: String,
    /// Whose documents may be read: the owner's, plus the Household's for
    /// a personal agent.
    pub readable: Vec<String>,
}

impl Scope {
    /// Where the owner's Google refresh token lives in the secret store.
    pub fn google_grant_key(&self) -> String {
        grant_key(&self.owner)
    }
}

pub fn grant_key(owner: &str) -> String {
    format!(
        "USER_{}__GOOGLE__REFRESH_TOKEN",
        owner.to_ascii_uppercase().replace('-', "_")
    )
}

/// The agent's scope, or why it has none (its owner was removed).
pub fn scope(gw: &Gateway, agent: &Agent) -> Result<Scope, String> {
    if agent.owner == HOUSEHOLD {
        return Ok(Scope {
            owner: HOUSEHOLD.into(),
            topic: gw.settings.ntfy_topic.clone(),
            readable: vec![HOUSEHOLD.into()],
        });
    }
    match gw.users.get(&agent.owner) {
        Some(u) => Ok(Scope {
            owner: u.name.clone(),
            topic: u.topic,
            readable: vec![agent.owner.clone(), HOUSEHOLD.into()],
        }),
        None => Err("this agent's owner was removed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Audit;
    use crate::calls::tests::gateway;
    use sesh_core::{GatewaySettings, User, UsersFile};

    fn agent(owner: &str) -> Agent {
        Agent {
            name: "a".into(),
            token_sha256: "0".repeat(64),
            connectors: vec!["mail".into()],
            runtime: None,
            owner: owner.into(),
        }
    }

    #[test]
    fn scope_comes_from_the_owner_only() {
        let mut gw = gateway(
            Audit::memory(),
            GatewaySettings {
                ntfy_topic: "hh-topic".into(),
                ..Default::default()
            },
        );
        gw.users = crate::Users::fixed(UsersFile {
            users: vec![User {
                name: "ana".into(),
                topic: "ana-topic".into(),
            }],
        });
        let s = scope(&gw, &agent("ana")).unwrap();
        assert_eq!(s.topic, "ana-topic");
        assert_eq!(s.readable, vec!["ana".to_string(), "household".into()]);
        assert_eq!(s.google_grant_key(), "USER_ANA__GOOGLE__REFRESH_TOKEN");
        let h = scope(&gw, &agent("household")).unwrap();
        assert_eq!((h.topic.as_str(), h.readable.len()), ("hh-topic", 1));
        assert_eq!(
            scope(&gw, &agent("ben")).unwrap_err(),
            "this agent's owner was removed"
        );
        assert_eq!(grant_key("ben-2"), "USER_BEN_2__GOOGLE__REFRESH_TOKEN");
    }
}
