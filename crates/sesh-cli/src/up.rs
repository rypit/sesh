//! `sesh up` and `sesh down` (contracts/cli.md).
//!
//! Brings up postgres, redis and the alias router, creates (or updates) the
//! restricted router key Open WebUI uses, then starts Open WebUI. Open
//! WebUI never receives the router's master key.

use crate::config::Config;
use crate::host::compose::Compose;
use crate::host::http::Http;
use crate::init::is_stub;
use crate::render;
use crate::secrets::{self, Secrets};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;

pub const COMPOSE_YAML: &str = include_str!("../../../deploy/compose.yaml");
pub const COMPOSE_STUB_YAML: &str = include_str!("../../../deploy/compose.stub.yaml");
pub const POSTGRES_INIT_SQL: &str = include_str!("../../../deploy/postgres/init.sql");

/// Writes the Compose files into `$SESH_DATA_DIR/compose/` and returns
/// the files to pass to `docker compose`.
pub fn write_compose_files(dir: &Path, stub: bool) -> Result<Vec<std::path::PathBuf>> {
    let cdir = dir.join("compose");
    std::fs::create_dir_all(&cdir)?;
    std::fs::write(cdir.join("compose.yaml"), COMPOSE_YAML)?;
    std::fs::write(cdir.join("postgres-init.sql"), POSTGRES_INIT_SQL)?;
    let mut files = vec![cdir.join("compose.yaml")];
    if stub {
        std::fs::write(cdir.join("compose.stub.yaml"), COMPOSE_STUB_YAML)?;
        files.push(cdir.join("compose.stub.yaml"));
    }
    Ok(files)
}

pub fn router_url(config: &Config) -> String {
    format!("http://127.0.0.1:{}", config.ports.router)
}

/// Runs `sesh up`; returns lines to print.
pub fn run(dir: &Path, compose: &dyn Compose, http: &dyn Http) -> Result<Vec<String>> {
    let config = Config::load(dir)?;
    let mut secrets = Secrets::load(dir)?;
    if secrets.get("LITELLM_MASTER_KEY").is_none() {
        bail!("secrets.env is incomplete; run `sesh init`");
    }
    let mut report = Vec::new();

    // The router reads litellm.yaml at start; `applied` records what the
    // running router last loaded, so a change made by `sesh init` is seen.
    let path = dir.join(render::FILE_NAME);
    let applied_path = dir.join("compose").join("litellm.applied");
    let rendered = render::litellm_config(&config);
    std::fs::write(&path, &rendered)?;
    let previous = std::fs::read_to_string(&applied_path).ok();
    let router_config_changed = previous.as_deref() != Some(rendered.as_str());
    write_compose_files(dir, is_stub(&config))?;

    let mut base = vec!["postgres", "redis", "litellm"];
    if is_stub(&config) {
        base.push("stub-llm");
    }
    compose.up(&base)?;
    if router_config_changed && previous.is_some() {
        // A bind-mounted file change isn't noticed by `up`; restart only the router.
        compose.restart("litellm")?;
        report.push("router config changed: restarted litellm".into());
    }
    std::fs::write(&applied_path, &rendered)?;

    let aliases: Vec<&str> = config.aliases.keys().map(String::as_str).collect();
    let master = secrets
        .get("LITELLM_MASTER_KEY")
        .unwrap_or_default()
        .to_string();
    let url = router_url(&config);
    match secrets.get(secrets::WEBUI_ROUTER_KEY) {
        None => {
            let key = create_client_key(http, &url, &master, "open-webui", &aliases)?;
            secrets.set(secrets::WEBUI_ROUTER_KEY, &key);
            secrets.save(dir)?;
            report.push("created Open WebUI's router key (limited to the aliases)".into());
        }
        Some(key) => update_client_key(http, &url, &master, key, &aliases)?,
    }

    compose.up(&["open-webui"])?;
    report.push(format!(
        "stack is up: Open WebUI on http://127.0.0.1:{}, router on {url}",
        config.ports.webui
    ));
    Ok(report)
}

pub fn down(compose: &dyn Compose) -> Result<()> {
    compose.down(false)
}

fn create_client_key(
    http: &dyn Http,
    url: &str,
    master: &str,
    alias: &str,
    models: &[&str],
) -> Result<String> {
    let body = json!({"key_alias": alias, "models": models});
    let r = http
        .post(&format!("{url}/key/generate"), Some(master), &body)
        .context("can't reach the router to create a key")?;
    if r.status != 200 {
        bail!("router refused to create a key ({}): {}", r.status, r.body);
    }
    r.body
        .get("key")
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("router's key response had no key")
}

fn update_client_key(
    http: &dyn Http,
    url: &str,
    master: &str,
    key: &str,
    models: &[&str],
) -> Result<()> {
    let r = http.post(
        &format!("{url}/key/update"),
        Some(master),
        &json!({"key": key, "models": models}),
    )?;
    if r.status != 200 {
        bail!(
            "router refused to update the client key ({}): {}",
            r.status,
            r.body
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::{FakeCompose, FakeHttp};
    use crate::init::{self, InitOptions};

    fn ready(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sesh-up-{name}-{}", std::process::id()));
        init::run(
            &dir,
            &InitOptions {
                stub: true,
                ..Default::default()
            },
        )
        .unwrap();
        dir
    }

    fn router() -> FakeHttp {
        FakeHttp::new(|_, url, body| {
            if url.ends_with("/key/generate") {
                assert_eq!(body.unwrap()["models"], json!(["agent-main"]));
                (200, json!({"key": "sk-client"}))
            } else {
                (200, json!({}))
            }
        })
    }

    #[test]
    fn first_up_creates_a_restricted_key_and_starts_webui_last() {
        let dir = ready("first");
        let compose = FakeCompose::default();
        run(&dir, &compose, &router()).unwrap();
        assert_eq!(
            *compose.calls.borrow(),
            vec!["up postgres redis litellm stub-llm", "up open-webui"]
        );
        assert_eq!(
            Secrets::load(&dir).unwrap().get("WEBUI_ROUTER_KEY"),
            Some("sk-client")
        );
    }

    #[test]
    fn later_up_updates_the_key_and_restarts_only_a_changed_router() {
        let dir = ready("later");
        run(&dir, &FakeCompose::default(), &router()).unwrap();
        let compose = FakeCompose::default();
        let http = router();
        run(&dir, &compose, &http).unwrap();
        assert!(
            !compose
                .calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("restart"))
        );
        assert!(
            http.calls
                .borrow()
                .iter()
                .any(|c| c.ends_with("/key/update"))
        );

        init::run(
            &dir,
            &InitOptions {
                inference_url: Some("http://srv/v1".into()),
                model: Some("m".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let compose = FakeCompose::default();
        run(&dir, &compose, &router()).unwrap();
        assert_eq!(
            *compose.calls.borrow(),
            vec![
                "up postgres redis litellm",
                "restart litellm",
                "up open-webui"
            ]
        );
    }
}
