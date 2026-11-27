//! Types shared by the gateway and the CLI (specs/002-gateway-core/data-model.md).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub const AGENTS_FILE: &str = "agents.toml";

/// One agent: its name, the SHA-256 of its token, and the connectors it may use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub name: String,
    pub token_sha256: String,
    pub connectors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsFile {
    #[serde(default, rename = "agent")]
    pub agents: Vec<Agent>,
}

pub fn valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
}

/// Hex SHA-256 of a token.
pub fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

impl AgentsFile {
    pub fn parse(text: &str) -> Result<Self> {
        let f: AgentsFile = toml::from_str(text).context("agents.toml is not valid")?;
        f.validate()?;
        Ok(f)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(AGENTS_FILE);
        if !path.exists() {
            return Ok(AgentsFile::default());
        }
        Self::parse(&std::fs::read_to_string(&path)?)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        std::fs::write(dir.join(AGENTS_FILE), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.agents {
            if !valid_agent_name(&a.name) {
                bail!(
                    "agent name {:?} must be 1-32 characters of a-z, 0-9 and -",
                    a.name
                );
            }
            if !seen.insert(&a.name) {
                bail!("agent {} is listed twice", a.name);
            }
            if a.token_sha256.len() != 64 || !a.token_sha256.chars().all(|c| c.is_ascii_hexdigit())
            {
                bail!("agent {} has an invalid token hash", a.name);
            }
            if a.connectors.is_empty() {
                bail!("agent {} has no connectors", a.name);
            }
        }
        Ok(())
    }

    pub fn by_token(&self, token: &str) -> Option<&Agent> {
        let h = token_hash(token);
        self.agents.iter().find(|a| a.token_sha256 == h)
    }

    pub fn get(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.name == name)
    }
}

/// A method a connector offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodDescriptor {
    pub description: String,
    /// JSON Schema subset (see the gateway's `schema` module).
    pub input: Value,
    #[serde(default)]
    pub requires_approval: bool,
}

/// A connector: a namespace of methods, as the executor and agents see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorDescriptor {
    pub name: String,
    pub instructions: String,
    pub methods: BTreeMap<String, MethodDescriptor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str) -> Agent {
        Agent {
            name: name.into(),
            token_sha256: token_hash(&format!("tok-{name}")),
            connectors: vec!["search".into()],
        }
    }

    #[test]
    fn finds_agents_by_token_hash_only() {
        let f = AgentsFile {
            agents: vec![agent("alice"), agent("bob")],
        };
        assert_eq!(f.by_token("tok-bob").unwrap().name, "bob");
        assert!(f.by_token("tok-carol").is_none());
        assert!(
            f.by_token(&f.agents[0].token_sha256).is_none(),
            "the hash is not a token"
        );
    }

    #[test]
    fn round_trips_and_validates() {
        let f = AgentsFile {
            agents: vec![agent("alice")],
        };
        let text = toml::to_string_pretty(&f).unwrap();
        assert_eq!(AgentsFile::parse(&text).unwrap(), f);
        let mut dup = f.clone();
        dup.agents.push(agent("alice"));
        assert!(dup.validate().is_err());
        let mut bad = f.clone();
        bad.agents[0].name = "Alice".into();
        assert!(bad.validate().is_err());
        let mut none = f;
        none.agents[0].connectors.clear();
        assert!(none.validate().is_err());
    }

    #[test]
    fn hash_is_hex_sha256() {
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

/// Gateway settings (data-model.md, Gateway settings). The CLI keeps them in
/// `sesh.toml [gateway]` and renders `gateway.toml` for the gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewaySettings {
    #[serde(default = "d_deadline")]
    pub deadline_secs: u64,
    #[serde(default = "d_calls")]
    pub max_calls: u32,
    #[serde(default = "d_result")]
    pub max_result_bytes: usize,
    #[serde(default)]
    pub home: Option<Home>,
    #[serde(default)]
    pub upstreams: Upstreams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Home {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upstreams {
    #[serde(default = "d_searxng")]
    pub searxng: String,
    #[serde(default = "d_kiwix")]
    pub kiwix: String,
    #[serde(default = "d_open_meteo")]
    pub open_meteo: String,
    #[serde(default = "d_nominatim")]
    pub nominatim: String,
}

fn d_deadline() -> u64 {
    30
}
fn d_calls() -> u32 {
    50
}
fn d_result() -> usize {
    65536
}
fn d_searxng() -> String {
    "http://searxng:8080".into()
}
fn d_kiwix() -> String {
    "http://kiwix:8080".into()
}
fn d_open_meteo() -> String {
    "https://api.open-meteo.com".into()
}
fn d_nominatim() -> String {
    "https://nominatim.openstreetmap.org".into()
}

impl Default for Upstreams {
    fn default() -> Self {
        Upstreams {
            searxng: d_searxng(),
            kiwix: d_kiwix(),
            open_meteo: d_open_meteo(),
            nominatim: d_nominatim(),
        }
    }
}

impl Default for GatewaySettings {
    fn default() -> Self {
        GatewaySettings {
            deadline_secs: d_deadline(),
            max_calls: d_calls(),
            max_result_bytes: d_result(),
            home: None,
            upstreams: Upstreams::default(),
        }
    }
}

impl GatewaySettings {
    pub fn validate(&self) -> Result<()> {
        if !(1..=600).contains(&self.deadline_secs) {
            bail!("gateway.deadline_secs must be 1-600");
        }
        if !(1..=1000).contains(&self.max_calls) {
            bail!("gateway.max_calls must be 1-1000");
        }
        if !(1024..=1_048_576).contains(&self.max_result_bytes) {
            bail!("gateway.max_result_bytes must be 1 KiB-1 MiB");
        }
        if let Some(h) = &self.home
            && (!(-90.0..=90.0).contains(&h.latitude) || !(-180.0..=180.0).contains(&h.longitude))
        {
            bail!("gateway.home is not a valid latitude/longitude");
        }
        Ok(())
    }
}
