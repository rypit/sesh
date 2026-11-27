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
    /// Credentials the connector needs, by name (specs/004 data-model.md).
    /// Only the gateway ever holds their values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<String>,
}

/// A credential name: `[A-Z][A-Z0-9_]{0,63}`.
pub fn valid_credential_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('A'..='Z'))
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Where a credential lives in the secret store: `<CONNECTOR>__<NAME>`.
pub fn credential_key(connector: &str, name: &str) -> String {
    format!(
        "{}__{name}",
        connector.to_ascii_uppercase().replace('-', "_")
    )
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

    #[test]
    fn credential_names_and_keys() {
        assert!(valid_credential_name("TOKEN") && valid_credential_name("API_KEY_2"));
        for bad in ["", "token", "1TOKEN", "TO-KEN", &"A".repeat(65)] {
            assert!(!valid_credential_name(bad), "{bad}");
        }
        assert_eq!(credential_key("testcred", "TOKEN"), "TESTCRED__TOKEN");
    }

    #[test]
    fn approval_settings_default_and_validate() {
        let s = GatewaySettings::default();
        assert_eq!(s.approvals_base_url, "http://127.0.0.1:7002");
        assert_eq!((s.approval_ttl_secs, s.form_ttl_secs), (900, 1800));
        assert_eq!((s.approvals_pending_max, s.approvals_per_hour), (10, 30));
        assert!(s.validate().is_ok());
        let bad = GatewaySettings {
            ntfy_topic: "a b".into(),
            ..GatewaySettings::default()
        };
        assert!(bad.validate().is_err());
        let bad = GatewaySettings {
            approval_ttl_secs: 1,
            ..GatewaySettings::default()
        };
        assert!(bad.validate().is_err());
        let old: GatewaySettings = toml::from_str("deadline_secs = 30").unwrap();
        assert_eq!(
            old.approval_ttl_secs, 900,
            "older gateway.toml files still load"
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
    /// Base of the approval and form links the person opens (specs/004).
    #[serde(default = "d_approvals_base")]
    pub approvals_base_url: String,
    #[serde(default = "d_approval_ttl")]
    pub approval_ttl_secs: u64,
    #[serde(default = "d_form_ttl")]
    pub form_ttl_secs: u64,
    #[serde(default = "d_pending_max")]
    pub approvals_pending_max: u32,
    #[serde(default = "d_per_hour")]
    pub approvals_per_hour: u32,
    /// The person's ntfy topic; empty until `sesh init` generates one.
    #[serde(default)]
    pub ntfy_topic: String,
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
    /// The test connectors' upstream (stub mode only).
    #[serde(default = "d_test")]
    pub test: String,
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
fn d_test() -> String {
    "http://stub-llm:8000".into()
}
fn d_approvals_base() -> String {
    "http://127.0.0.1:7002".into()
}
fn d_approval_ttl() -> u64 {
    900
}
fn d_form_ttl() -> u64 {
    1800
}
fn d_pending_max() -> u32 {
    10
}
fn d_per_hour() -> u32 {
    30
}

impl Default for Upstreams {
    fn default() -> Self {
        Upstreams {
            searxng: d_searxng(),
            kiwix: d_kiwix(),
            open_meteo: d_open_meteo(),
            nominatim: d_nominatim(),
            test: d_test(),
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
            approvals_base_url: d_approvals_base(),
            approval_ttl_secs: d_approval_ttl(),
            form_ttl_secs: d_form_ttl(),
            approvals_pending_max: d_pending_max(),
            approvals_per_hour: d_per_hour(),
            ntfy_topic: String::new(),
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
        if !(self.approvals_base_url.starts_with("http://")
            || self.approvals_base_url.starts_with("https://"))
        {
            bail!("gateway.approvals_base_url must be an http(s) URL");
        }
        if !(10..=86_400).contains(&self.approval_ttl_secs)
            || !(10..=86_400).contains(&self.form_ttl_secs)
        {
            bail!("gateway.approval_ttl_secs and form_ttl_secs must be 10 s-1 day");
        }
        if self.approvals_pending_max == 0 || self.approvals_per_hour == 0 {
            bail!("gateway approval limits must be at least 1");
        }
        if !self.ntfy_topic.is_empty()
            && !self
                .ntfy_topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("gateway.ntfy_topic may hold only letters, digits, - and _");
        }
        if let Some(h) = &self.home
            && (!(-90.0..=90.0).contains(&h.latitude) || !(-180.0..=180.0).contains(&h.longitude))
        {
            bail!("gateway.home is not a valid latitude/longitude");
        }
        Ok(())
    }
}
