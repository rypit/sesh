//! `sesh init`: create the data directory, `sesh.toml`, `secrets.env` and
//! the rendered router config (contracts/cli.md). Idempotent.

use crate::config::{Config, REQUIRED_ALIAS};
use crate::render;
use crate::secrets::{self, Secrets};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// The inference setting used by `--stub`: the stub service on the stack's
/// own network.
pub const STUB_URL: &str = "http://stub-llm:8000/v1";
pub const STUB_MODEL: &str = "stub-model";
pub const DEFAULT_ADMIN_EMAIL: &str = "admin@sesh.local";

#[derive(Debug, Default)]
pub struct InitOptions {
    pub inference_url: Option<String>,
    pub model: Option<String>,
    pub stub: bool,
    pub admin_email: Option<String>,
}

/// Where sesh keeps its files: `$SESH_DATA_DIR`, else
/// `~/.local/share/sesh`.
pub fn data_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SESH_DATA_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    let Ok(home) = std::env::var("HOME") else {
        bail!("HOME is not set; set SESH_DATA_DIR");
    };
    Ok(PathBuf::from(home).join(".local/share/sesh"))
}

pub fn is_stub(config: &Config) -> bool {
    config.inference.base_url == STUB_URL
}

/// Runs `sesh init` and returns the lines to print (never secret values).
pub fn run(dir: &Path, opts: &InitOptions) -> Result<Vec<String>> {
    let mut report = Vec::new();
    secrets::ensure_private_dir(dir)?;

    let (url, model) = if opts.stub {
        (Some(STUB_URL.to_string()), Some(STUB_MODEL.to_string()))
    } else {
        (opts.inference_url.clone(), opts.model.clone())
    };

    let existing = dir.join(crate::config::FILE_NAME).exists();
    let mut config = if existing {
        Config::load(dir)?
    } else {
        let (Some(u), Some(m)) = (&url, &model) else {
            bail!("no sesh.toml yet: pass --inference-url and --model (or --stub)");
        };
        Config::new(u, m)
    };
    let before = config.clone();
    if let Some(u) = url {
        config.inference.base_url = u;
    }
    if let Some(m) = model {
        config.aliases.insert(REQUIRED_ALIAS.to_string(), m);
    }
    config.validate()?;
    if !existing || config != before {
        config.save(dir)?;
        report.push(format!(
            "wrote {} (inference {}, agent-main = {})",
            dir.join(crate::config::FILE_NAME).display(),
            config.inference.base_url,
            config.aliases[REQUIRED_ALIAS]
        ));
    }

    let mut secrets = Secrets::load(dir)?;
    let before = secrets.clone();
    for key in secrets.fill_missing()? {
        report.push(format!("generated {key}"));
    }
    match (&opts.admin_email, secrets.get(secrets::WEBUI_ADMIN_EMAIL)) {
        (Some(email), _) => secrets.set(secrets::WEBUI_ADMIN_EMAIL, email),
        (None, None) => secrets.set(secrets::WEBUI_ADMIN_EMAIL, DEFAULT_ADMIN_EMAIL),
        (None, Some(_)) => {}
    }
    if secrets != before {
        secrets.save(dir)?;
        report.push(format!(
            "wrote {} (mode 0600; values not shown)",
            dir.join(secrets::FILE_NAME).display()
        ));
    }

    let rendered = render::litellm_config(&config);
    let path = dir.join(render::FILE_NAME);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(rendered.as_str()) {
        std::fs::write(&path, rendered)?;
        report.push(format!("wrote {}", path.display()));
    }
    if report.is_empty() {
        report.push("nothing to change".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sesh-init-{name}-{}", std::process::id()))
    }

    #[test]
    fn needs_inference_first_time() {
        assert!(run(&tempdir("needs"), &InitOptions::default()).is_err());
    }

    #[test]
    fn creates_everything_then_is_idempotent() {
        let dir = tempdir("idem");
        let opts = InitOptions {
            stub: true,
            ..Default::default()
        };
        let first = run(&dir, &opts).unwrap();
        assert!(first.iter().any(|l| l.contains("sesh.toml")));
        assert!(dir.join("secrets.env").exists() && dir.join("litellm.yaml").exists());
        let secrets = Secrets::load(&dir).unwrap();
        assert_eq!(secrets.get("WEBUI_ADMIN_EMAIL"), Some(DEFAULT_ADMIN_EMAIL));
        // No secret value ever reaches the report.
        for key in crate::secrets::GENERATED {
            let value = secrets.get(key).unwrap();
            assert!(first.iter().all(|l| !l.contains(value)));
        }
        assert_eq!(
            run(&dir, &opts).unwrap(),
            vec!["nothing to change".to_string()]
        );
        assert_eq!(Secrets::load(&dir).unwrap(), secrets);
    }

    #[test]
    fn switching_inference_changes_only_config_and_router() {
        let dir = tempdir("switch");
        run(
            &dir,
            &InitOptions {
                stub: true,
                ..Default::default()
            },
        )
        .unwrap();
        let secrets = Secrets::load(&dir).unwrap();
        let report = run(
            &dir,
            &InitOptions {
                inference_url: Some("http://srv:8080/v1".into()),
                model: Some("qwen3".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.len(), 2, "{report:?}");
        assert_eq!(Secrets::load(&dir).unwrap(), secrets);
        let y = std::fs::read_to_string(dir.join("litellm.yaml")).unwrap();
        assert!(y.contains("openai/qwen3"));
    }
}
