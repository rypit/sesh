//! `sesh.toml`: the operator's inference setting, model aliases and ports
//! (data-model.md, Config). Unknown keys are an error.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const FILE_NAME: &str = "sesh.toml";
pub const REQUIRED_ALIAS: &str = "agent-main";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub inference: Inference,
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub ports: Ports,
    /// The gateway's limits, home location and upstreams (feature 002).
    #[serde(default)]
    pub gateway: sesh_core::GatewaySettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inference {
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    #[serde(default = "default_webui_port")]
    pub webui: u16,
    #[serde(default = "default_router_port")]
    pub router: u16,
}

impl Default for Ports {
    fn default() -> Self {
        Ports {
            webui: default_webui_port(),
            router: default_router_port(),
        }
    }
}

fn default_timeout() -> u64 {
    120
}
fn default_webui_port() -> u16 {
    3000
}
fn default_router_port() -> u16 {
    4000
}

impl Config {
    /// A config with one inference server and `agent-main` mapped to `model`.
    pub fn new(base_url: &str, model: &str) -> Self {
        let mut aliases = BTreeMap::new();
        aliases.insert(REQUIRED_ALIAS.to_string(), model.to_string());
        Config {
            inference: Inference {
                base_url: base_url.to_string(),
                api_key_env: None,
                timeout_secs: default_timeout(),
            },
            aliases,
            ports: Ports::default(),
            gateway: sesh_core::GatewaySettings::default(),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Config = toml::from_str(text).context("sesh.toml is not valid")?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE_NAME);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("can't read {}; run `sesh init` first", path.display()))?;
        Self::parse(&text)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        std::fs::write(dir.join(FILE_NAME), self.to_toml()?)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let url = &self.inference.base_url;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            bail!("inference.base_url must start with http:// or https:// (got {url:?})");
        }
        if url.contains("litellm:") {
            bail!("inference.base_url must point at the inference server, not the alias router");
        }
        if let Some(name) = &self.inference.api_key_env
            && (name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            bail!("inference.api_key_env must be an environment variable name");
        }
        if !(1..=600).contains(&self.inference.timeout_secs) {
            bail!("inference.timeout_secs must be between 1 and 600");
        }
        if !self.aliases.contains_key(REQUIRED_ALIAS) {
            bail!("aliases must include {REQUIRED_ALIAS}");
        }
        for (alias, model) in &self.aliases {
            let ok = !alias.is_empty()
                && alias.len() <= 40
                && alias
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !ok {
                bail!("alias {alias:?} must be 1-40 characters of a-z, 0-9 and -");
            }
            if model.trim().is_empty() {
                bail!("alias {alias} has no model name");
            }
        }
        self.gateway.validate()?;
        if self.ports.webui == self.ports.router {
            bail!("ports.webui and ports.router must differ");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[inference]
base_url = "http://192.168.1.20:11434/v1"

[aliases]
agent-main = "qwen3"
"#;

    #[test]
    fn parses_with_defaults() {
        let c = Config::parse(GOOD).unwrap();
        assert_eq!(c.inference.timeout_secs, 120);
        assert_eq!(c.ports, Ports::default());
        assert_eq!(c.aliases["agent-main"], "qwen3");
    }

    #[test]
    fn round_trips() {
        let c = Config::new("http://h:1/v1", "m");
        assert_eq!(Config::parse(&c.to_toml().unwrap()).unwrap(), c);
    }

    #[test]
    fn rejects_unknown_keys() {
        let bad = format!("{GOOD}\n[extra]\nx = 1\n");
        assert!(Config::parse(&bad).is_err());
    }

    #[test]
    fn requires_agent_main() {
        let bad = GOOD.replace("agent-main", "other");
        assert!(Config::parse(&bad).is_err());
    }

    #[test]
    fn rejects_bad_values() {
        for (from, to) in [
            ("http://192.168.1.20:11434/v1", "ftp://x"),
            ("http://192.168.1.20:11434/v1", "http://litellm:4000/v1"),
            ("agent-main", "Agent_Main"),
            ("\"qwen3\"", "\" \""),
        ] {
            assert!(Config::parse(&GOOD.replace(from, to)).is_err(), "{to}");
        }
        let mut c = Config::new("http://h/v1", "m");
        c.inference.timeout_secs = 0;
        assert!(c.validate().is_err());
        c.inference.timeout_secs = 10;
        c.inference.api_key_env = Some("NOT A NAME".into());
        assert!(c.validate().is_err());
        c.inference.api_key_env = None;
        c.ports.router = c.ports.webui;
        assert!(c.validate().is_err());
    }
}
