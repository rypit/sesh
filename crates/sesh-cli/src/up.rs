//! `sesh up` and `sesh down` (contracts/cli.md of features 001 and 002).
//!
//! Order: missing passwords → postgres → the migration (the service
//! logins, their databases, the audit log, then each service database
//! handed to its login: specs/007) → each service's Redis, the alias
//! router, search, the gateway and one executor per agent → the
//! restricted router key for Open WebUI → Open WebUI. Open WebUI never
//! receives the router's master key; executors receive no secrets at all.

use crate::agent;
use crate::config::Config;
use crate::host::compose::{Compose, NetworkRemoval};
use crate::host::http::Http;
use crate::init::is_stub;
use crate::render;
use crate::secrets::{self, Secrets};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sesh_core::{AgentsFile, GatewaySettings, Home};
use std::path::{Path, PathBuf};

pub const COMPOSE_YAML: &str = include_str!("../../../deploy/compose.yaml");
pub const COMPOSE_STUB_YAML: &str = include_str!("../../../deploy/compose.stub.yaml");
pub const POSTGRES_INIT_SQL: &str = include_str!("../../../deploy/postgres/init.sql");
pub const GATEWAY_SQL: &str = include_str!("../../../deploy/postgres/gateway.sql");
/// Run in each service database after [`GATEWAY_SQL`] (specs/007 R2).
pub const OWNERS_SQL: &str = include_str!("../../../deploy/postgres/owners.sql");
pub const SEARXNG_SETTINGS: &str = include_str!("../../../deploy/searxng/settings.yml");

const KIWIX_IMAGE: &str = "ghcr.io/kiwix/kiwix-serve:3.8.2@sha256:e478554a9920412eeb5ce1d73f24e4b28445ebed233187cdd03c623c32be46a5";
/// Each adopted service with its own Postgres login (specs/007 R1,
/// ADR 0010): the role, which is also its database's name, and the
/// `secrets.env` key of its password. `gateway.sql` makes each one.
pub const SERVICE_DATABASES: [(&str, &str); 5] = [
    ("openwebui", "OPENWEBUI_DB_PASSWORD"),
    ("litellm", "LITELLM_DB_PASSWORD"),
    ("infisical", "INFISICAL_DB_PASSWORD"),
    ("paperless", "PAPERLESS_DB_PASSWORD"),
    ("miniflux", "MINIFLUX_DB_PASSWORD"),
];

/// The stub's fake web sites (specs/010 R8), as network aliases of the stub.
pub const STUB_SITES: [&str; 3] = ["fake-news.test", "fake-bank.test", "fake-shop.test"];

/// Where the stub serves the fake upstreams (stub mode only).
const STUB_UPSTREAM: &str = "http://stub-llm:8000";

fn has_zim_files(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("kiwix"))
        .map(|rd| {
            rd.flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "zim"))
        })
        .unwrap_or(false)
}

/// The per-agent overlay: one executor and one internal network per agent,
/// both joined by the gateway; plus Kiwix when ZIM files exist. Pure.
pub fn generated_overlay(agents: &AgentsFile, kiwix: bool) -> String {
    let mut services = String::new();
    let mut networks = String::new();
    let mut gateway_nets = Vec::new();
    let mut volumes = String::new();
    let mut litellm_nets = vec![];
    let mut urls = vec!["http://litellm:4000/v1".to_string()];
    let mut keys = vec![format!("${{{}:-pending}}", secrets::WEBUI_ROUTER_KEY)];
    // Agent runtimes (specs/009 research.md R4-R6), reached by Open WebUI
    // through the gateway's chat listener (specs/010 R2).
    for a in agents.agents.iter().filter(|a| a.runtime.is_some()) {
        let n = &a.name;
        services.push_str(&crate::hermes::compose_service(n));
        let [own, model] = crate::hermes::networks(n);
        for net in [&own, &model] {
            networks.push_str(&format!("  {net}:\n    internal: true\n"));
        }
        gateway_nets.push(format!(
            "      {own}:\n        aliases: [{}]\n",
            crate::hermes::gateway_alias(n)
        ));
        litellm_nets.push(model);
        volumes.push_str(&format!("  {}-data: {{}}\n", crate::hermes::service(n)));
        urls.push(crate::hermes::chat_url(n));
        keys.push(format!(
            "${{{}:-pending}}",
            secrets::agent_key(n, secrets::API_KEY)
        ));
    }
    if !litellm_nets.is_empty() {
        gateway_nets.push(format!(
            "      chat:\n        aliases: [{}]\n",
            crate::hermes::CHAT_ALIAS
        ));
        services.push_str(&format!(
            "  litellm:\n    networks: [{}]\n",
            litellm_nets.join(", ")
        ));
        services.push_str(&format!(
            "  open-webui:\n    environment:\n      OPENAI_API_BASE_URLS: \"{}\"\n      OPENAI_API_KEYS: \"{}\"\n      ENABLE_FORWARD_USER_INFO_HEADERS: \"true\"\n      FORWARD_USER_INFO_HEADER_JWT_SECRET: ${{{}:?run sesh up}}\n",
            urls.join(";"),
            keys.join(";"),
            secrets::WEBUI_JWT_SECRET
        ));
    }
    // Agent browsers (specs/010 research.md R3, R4): one network, shared
    // only with the gateway, whose alias there is the egress proxy.
    for a in agents
        .agents
        .iter()
        .filter(|a| a.connectors.iter().any(|c| c == "browser"))
    {
        let n = &a.name;
        services.push_str(&browser_service(n));
        networks.push_str(&format!("  browser-{n}:\n    internal: true\n"));
        gateway_nets.push(format!(
            "      browser-{n}:\n        aliases: [gateway-browser-{n}]\n"
        ));
        volumes.push_str(&format!("  browser-{n}-profile: {{}}\n"));
    }
    for a in &agents.agents {
        let n = &a.name;
        services.push_str(&format!(
            "  executor-{n}:\n    image: ${{SESH_EXECUTOR_IMAGE:-sesh-executor:dev}}\n    environment:\n      SESH_GATEWAY_ADDRESS: gateway:7001\n    networks: [exec-{n}]\n    read_only: true\n    tmpfs: [/var/lib/sesh-executor]\n    cap_drop: [ALL]\n    security_opt: [\"no-new-privileges:true\"]\n    mem_limit: 512m\n    pids_limit: 256\n    healthcheck:\n      test: [\"CMD\", \"/usr/local/bin/sesh-executor-supervisor\", \"health\"]\n      interval: 5s\n      timeout: 5s\n      retries: 30\n    restart: unless-stopped\n"
        ));
        networks.push_str(&format!("  exec-{n}:\n    internal: true\n"));
        gateway_nets.push(format!("      exec-{n}: {{}}\n"));
    }
    if kiwix {
        services.push_str(&format!(
            "  kiwix:\n    image: {KIWIX_IMAGE}\n    command: [\"*.zim\"]\n    volumes:\n      - ${{SESH_DATA_DIR:?}}/kiwix:/data:ro\n    networks: [search]\n    restart: unless-stopped\n"
        ));
    }
    let mut out = String::from(
        "# Generated by `sesh up` from gateway/agents.toml. Do not edit.\nservices:\n",
    );
    if !gateway_nets.is_empty() {
        // Compose merges a service's `networks` across files, so this adds
        // the executor networks to the gateway's own (specs/008).
        out.push_str(&format!(
            "  gateway:\n    networks:\n{}",
            gateway_nets.concat()
        ));
    }
    out.push_str(&services);
    if services.is_empty() && gateway_nets.is_empty() {
        out = String::from("# Generated by `sesh up`: no agents yet.\nservices: {}\n");
    }
    if !networks.is_empty() {
        out.push_str("networks:\n");
        out.push_str(&networks);
    }
    if !volumes.is_empty() {
        out.push_str("volumes:\n");
        out.push_str(&volumes);
    }
    out
}

/// An agent's browser container (specs/010 research.md R3): Chromium,
/// hardened, its profile in a volume, its proxy the gateway.
pub fn browser_service(agent: &str) -> String {
    format!(
        "  browser-{agent}:\n    image: ${{SESH_BROWSER_IMAGE:-sesh-browser:dev}}\n    environment:\n      SESH_PROXY: http://gateway-browser-{agent}:7004\n      SESH_BROWSER_FLAGS: ${{SESH_BROWSER_FLAGS:-}}\n    volumes:\n      - browser-{agent}-profile:/profile\n    networks: [browser-{agent}]\n    read_only: true\n    tmpfs: [/tmp, \"/home/browser:uid=10001,gid=10001\"]\n    shm_size: 512m\n    cap_drop: [ALL]\n    security_opt: [\"no-new-privileges:true\"]\n    mem_limit: 1536m\n    pids_limit: 512\n    depends_on:\n      gateway:\n        condition: service_healthy\n    restart: unless-stopped\n"
    )
}

/// The gateway's alias on each agent runtime's network, for `gateway.toml`.
pub fn agent_listen(agents: &AgentsFile) -> Vec<String> {
    agents
        .agents
        .iter()
        .filter(|a| a.runtime.is_some())
        .map(|a| crate::hermes::gateway_alias(&a.name))
        .collect()
}

/// The gateway's settings as it should run them: in stub mode every
/// upstream is the stub's fakes, so nothing reaches the internet.
pub fn effective_gateway(config: &Config) -> GatewaySettings {
    let mut g = config.gateway.clone();
    if is_stub(config) {
        for u in [
            &mut g.upstreams.searxng,
            &mut g.upstreams.kiwix,
            &mut g.upstreams.open_meteo,
            &mut g.upstreams.nominatim,
        ] {
            *u = STUB_UPSTREAM.into();
        }
        // Fake Google (specs/005 research.md R7); Paperless stays real.
        g.upstreams.google_auth = format!("{STUB_UPSTREAM}/fake/google/auth");
        g.upstreams.google_token = format!("{STUB_UPSTREAM}/fake/google/token");
        g.upstreams.google_revoke = format!("{STUB_UPSTREAM}/fake/google/revoke");
        for u in [
            &mut g.upstreams.gmail,
            &mut g.upstreams.calendar,
            &mut g.upstreams.people,
        ] {
            *u = format!("{STUB_UPSTREAM}/fake/google");
        }
        // The stub's fake sites (specs/010 R8): their private address is
        // allowed by the browser's egress proxy, in stub mode only.
        g.browser.egress_allow = STUB_SITES.iter().map(|s| s.to_string()).collect();
        g.browser.allow_http_login = true;
        // Tests wait for a takeover to time out; the household keeps 60 s.
        g.browser.wait_secs = 5;
        g.home.get_or_insert(Home {
            latitude: 59.91,
            longitude: 10.75,
        });
    }
    g
}

/// Writes `text` to `path` through a temporary file in the same directory,
/// readable by the container that mounts it (Postgres runs as its own user).
fn write_readable(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
        if let Some(d) = path.parent() {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Writes the Compose files and the gateway's config into the data dir and
/// returns the files to pass to `docker compose`.
pub fn write_compose_files(dir: &Path, config: &Config) -> Result<Vec<PathBuf>> {
    let cdir = dir.join("compose");
    std::fs::create_dir_all(&cdir)?;
    std::fs::write(cdir.join("compose.yaml"), COMPOSE_YAML)?;
    std::fs::write(cdir.join("postgres-init.sql"), POSTGRES_INIT_SQL)?;
    std::fs::write(cdir.join("searxng-settings.yml"), SEARXNG_SETTINGS)?;
    // Postgres's login rules (specs/008 R10): local administration only,
    // until `sesh up` has read the networks' ranges.
    let hba_dir = dir.join(crate::pg_hba::DIR);
    std::fs::create_dir_all(&hba_dir)?;
    let hba = hba_dir.join(crate::pg_hba::FILE);
    if !hba.exists() {
        write_readable(&hba, &crate::pg_hba::initial())?;
    }
    let gdir = agent::gateway_dir(dir);
    std::fs::create_dir_all(&gdir)?;
    let agents = agent::load(dir)?;
    if !gdir.join(sesh_core::AGENTS_FILE).exists() {
        agents.save(&gdir)?;
    }
    write_runtime_files(dir, &agents)?;
    let mut gw = effective_gateway(config);
    gw.agent_listen = agent_listen(&agents);
    gw.browser.agents = agents
        .agents
        .iter()
        .filter(|a| a.connectors.iter().any(|c| c == "browser"))
        .map(|a| a.name.clone())
        .collect();
    if agents.agents.iter().any(|a| a.runtime.is_some()) {
        gw.browser.chat_alias = crate::hermes::CHAT_ALIAS.into();
    }
    std::fs::write(gdir.join("gateway.toml"), toml::to_string_pretty(&gw)?)?;
    let mut files = vec![cdir.join("compose.yaml")];
    if is_stub(config) {
        std::fs::write(cdir.join("compose.stub.yaml"), COMPOSE_STUB_YAML)?;
        files.push(cdir.join("compose.stub.yaml"));
    }
    std::fs::write(
        cdir.join("generated.yaml"),
        generated_overlay(&agents, has_zim_files(dir)),
    )?;
    files.push(cdir.join("generated.yaml"));
    Ok(files)
}

/// Each agent runtime's `config.yaml` and credentials file (specs/009
/// research.md R7, R8), written only when they change. The router key is
/// `pending` until `sesh up` has created it; the runtime is recreated
/// when its credentials change. Returns the runtimes whose files changed.
pub fn write_runtime_files(dir: &Path, agents: &AgentsFile) -> Result<Vec<String>> {
    let secrets = Secrets::load(dir)?;
    let mut changed = vec![];
    for a in agents.agents.iter().filter(|a| a.runtime.is_some()) {
        let n = &a.name;
        let cfg = crate::hermes::write_if_changed(
            &crate::hermes::config_path(dir, n),
            &crate::hermes::config_text(n),
            false,
        )?;
        let token = std::fs::read_to_string(agent::token_path(dir, n))
            .with_context(|| format!("agent {n} has no token; remove and add it again"))?;
        let get = |what: &str| {
            secrets
                .get(&secrets::agent_key(n, what))
                .unwrap_or("pending")
                .to_string()
        };
        let env = crate::hermes::write_if_changed(
            &crate::hermes::env_path(dir, n),
            &crate::hermes::env_text(
                token.trim(),
                &get(secrets::ROUTER_KEY),
                &get(secrets::API_KEY),
            ),
            true,
        )?;
        if cfg || env {
            changed.push(crate::hermes::service(n));
        }
    }
    Ok(changed)
}

pub fn router_url(config: &Config) -> String {
    format!("http://127.0.0.1:{}", config.ports.router)
}

/// Restarts `service` when `content` differs from what it last loaded.
fn apply_if_changed(
    compose: &dyn Compose,
    stamp: &Path,
    content: &str,
    service: &str,
    report: &mut Vec<String>,
) -> Result<bool> {
    let previous = std::fs::read_to_string(stamp).ok();
    let restarted = previous.is_some() && previous.as_deref() != Some(content);
    if restarted {
        compose.restart(service)?;
        report.push(format!("{service} config changed: restarted {service}"));
    }
    std::fs::write(stamp, content)?;
    Ok(restarted)
}

/// The migration with its passwords as psql variables, for stdin only:
/// never a command line or a file. `services` is (role, password); after
/// the migration, each role's database is handed to it.
pub fn migration_sql(gateway_pw: &str, services: &[(&str, &str)]) -> Result<String> {
    let mut out = String::new();
    for (name, pw) in std::iter::once(("gateway", gateway_pw)).chain(services.iter().copied()) {
        // A value that could end the psql literal or line is refused, so
        // a hand-edited secrets.env can't inject SQL.
        if pw.is_empty() || pw.contains(['\'', '\\', '\n', '\r']) {
            bail!(
                "the {name} database password in secrets.env is empty or has a quote, backslash or line break; remove it and run `sesh up` to generate a new one"
            );
        }
        out.push_str(&format!("\\set {name}_pw '{pw}'\n"));
    }
    out.push_str(GATEWAY_SQL);
    for (role, _) in services {
        out.push_str(&format!("\\c {role}\n{OWNERS_SQL}"));
    }
    Ok(out)
}

/// Runs `sesh up`; returns lines to print.
pub fn run(dir: &Path, compose: &dyn Compose, http: &dyn Http) -> Result<Vec<String>> {
    let config = Config::load(dir)?;
    let mut secrets = Secrets::load(dir)?;
    let (Some(_), Some(gateway_pw)) = (
        secrets.get("LITELLM_MASTER_KEY"),
        secrets.get("GATEWAY_DB_PASSWORD"),
    ) else {
        bail!("secrets.env is incomplete; run `sesh init`");
    };
    let gateway_pw = gateway_pw.to_string();
    let mut report = Vec::new();
    // An install from before a feature lacks that feature's passwords
    // (specs/007 FR-007): make them, so upgrading is `sesh up` alone.
    let made = secrets.fill_missing()?;
    if !made.is_empty() {
        secrets.save(dir)?;
        report.extend(made.iter().map(|k| format!("generated {k}")));
    }

    let rendered = render::litellm_config(&config);
    std::fs::write(dir.join(render::FILE_NAME), &rendered)?;
    write_compose_files(dir, &config)?;
    // The approval channels (specs/004 R3, specs/005 R2): the Household's
    // topic, read by `person` and every user, and each user's own.
    let users = crate::user::load(dir)?;
    let hh = config.gateway.ntfy_topic.clone();
    let mut topics = vec![hh.clone()];
    let mut readers = vec![crate::ntfy::Reader {
        user: crate::ntfy::PERSON_USER.into(),
        password: secrets
            .get("NTFY_PERSON_PASSWORD")
            .context("NTFY_PERSON_PASSWORD missing; run `sesh init`")?
            .to_string(),
        topics: vec![hh.clone()],
    }];
    for u in &users.users {
        topics.push(u.topic.clone());
        readers.push(crate::ntfy::Reader {
            user: crate::ntfy::reader_name(&u.name),
            password: secrets
                .get(&crate::user::password_key(&u.name))
                .with_context(|| {
                    format!("no ntfy password for {}; run `sesh user add` again", u.name)
                })?
                .to_string(),
            topics: vec![u.topic.clone(), hh.clone()],
        });
    }
    let ntfy_changed = crate::ntfy::write_config(
        &dir.join("compose"),
        &topics,
        secrets
            .get("NTFY_GATEWAY_PASSWORD")
            .context("NTFY_GATEWAY_PASSWORD missing; run `sesh init`")?,
        &readers,
    )?;
    let gateway_toml = std::fs::read_to_string(agent::gateway_dir(dir).join("gateway.toml"))?;
    let agents = agent::load(dir)?;

    compose.up(&["postgres"])?;
    // Passwords go in on stdin as psql variables, never on a command line.
    let service_pws = SERVICE_DATABASES
        .iter()
        .map(|(role, key)| {
            secrets
                .get(key)
                .map(|pw| (*role, pw))
                .with_context(|| format!("{key} missing; run `sesh init`"))
        })
        .collect::<Result<Vec<_>>>()?;
    compose.exec_sql("sesh", &migration_sql(&gateway_pw, &service_pws)?)?;
    // Each database login only from its own network's range, before any
    // service starts (specs/008 research.md R10). Fails closed.
    let hba = dir.join(crate::pg_hba::DIR).join(crate::pg_hba::FILE);
    let rules = crate::pg_hba::render(&|n| compose.network_subnets(n))?;
    if std::fs::read_to_string(&hba).ok().as_deref() != Some(rules.as_str()) {
        write_readable(&hba, &rules)?;
        compose.exec_sql("sesh", "SELECT pg_reload_conf();")?;
        report.push(
            "database login rules updated (each login accepted only from its own network)".into(),
        );
    }

    let executors: Vec<String> = agents
        .agents
        .iter()
        .map(|a| format!("executor-{}", a.name))
        .collect();
    let mut base: Vec<&str> = vec![
        "redis-openwebui",
        "redis-infisical",
        "redis-paperless",
        "litellm",
        "searxng",
        "infisical",
        "ntfy",
        "paperless",
        "miniflux",
        "gateway",
    ];
    base.extend(executors.iter().map(String::as_str));
    if is_stub(&config) {
        base.push("stub-llm");
    }
    if has_zim_files(dir) {
        base.push("kiwix");
    }
    compose.up(&base)?;
    let cdir = dir.join("compose");
    // The secret store: bootstrap once, then the gateway restarts with its
    // own credentials (specs/004 research.md R5).
    let mut gateway_restarted = false;
    if crate::infisical::ensure(http, &mut secrets)? {
        secrets.save(dir)?;
        compose.restart("gateway")?;
        gateway_restarted = true;
        report.push("set up the secret store; only the gateway can read it".into());
    }
    if ntfy_changed && cdir.join("gateway.applied").exists() {
        compose.restart("ntfy")?;
        report.push("approval channel config changed: restarted ntfy".into());
    }
    apply_if_changed(
        compose,
        &cdir.join("litellm.applied"),
        &rendered,
        "litellm",
        &mut report,
    )?;
    gateway_restarted |= apply_if_changed(
        compose,
        &cdir.join("gateway.applied"),
        &gateway_toml,
        "gateway",
        &mut report,
    )?;

    let aliases: Vec<&str> = config.aliases.keys().map(String::as_str).collect();
    let master = secrets
        .get("LITELLM_MASTER_KEY")
        .unwrap_or_default()
        .to_string();
    let url = router_url(&config);
    // Open WebUI's key, and the one `sesh eval` uses (specs/003 research R9):
    // both limited to the aliases, never printed.
    for (name, label, who) in [
        (secrets::WEBUI_ROUTER_KEY, "open-webui", "Open WebUI's"),
        (secrets::EVAL_ROUTER_KEY, "sesh-eval", "the eval's"),
    ] {
        match secrets.get(name) {
            None => {
                let key = create_client_key(http, &url, &master, label, &aliases)?;
                secrets.set(name, &key);
                secrets.save(dir)?;
                report.push(format!("created {who} router key (limited to the aliases)"));
            }
            Some(key) => update_client_key(http, &url, &master, key, &aliases)?,
        }
    }

    // Agent runtimes (specs/009): each its own router key, then its
    // files, then the container, before Open WebUI lists it.
    let runtimes: Vec<&sesh_core::Agent> = agents
        .agents
        .iter()
        .filter(|a| a.runtime.is_some())
        .collect();
    for a in &runtimes {
        let name = secrets::agent_key(&a.name, secrets::ROUTER_KEY);
        match secrets.get(&name) {
            None => {
                let key =
                    create_client_key(http, &url, &master, &format!("agent-{}", a.name), &aliases)?;
                secrets.set(&name, &key);
                secrets.save(dir)?;
                report.push(format!(
                    "created {}'s router key (limited to the aliases)",
                    a.name
                ));
            }
            Some(key) => update_client_key(http, &url, &master, key, &aliases)?,
        }
    }
    // A removed agent's router key (its API key went with `sesh agent remove`).
    let keep: Vec<String> = runtimes
        .iter()
        .map(|a| secrets::agent_key(&a.name, secrets::ROUTER_KEY))
        .collect();
    for (k, v) in secrets.agent_router_keys() {
        if !keep.contains(&k) {
            delete_client_key(http, &url, &master, &v)?;
            secrets.remove(&k);
            secrets.save(dir)?;
            report.push(format!(
                "removed {}'s router key",
                k.trim_start_matches("AGENT_")
                    .trim_end_matches("_ROUTER_KEY")
                    .to_ascii_lowercase()
                    .replace('_', "-")
            ));
        }
    }
    write_runtime_files(dir, &agents)?;
    if !runtimes.is_empty() {
        let svcs: Vec<String> = runtimes
            .iter()
            .map(|a| crate::hermes::service(&a.name))
            .collect();
        compose.up(&svcs.iter().map(String::as_str).collect::<Vec<_>>())?;
        // Hermes parks its MCP connection if the gateway goes away
        // (research.md R12), so a restarted gateway restarts the runtimes.
        if gateway_restarted {
            for s in &svcs {
                compose.restart(s)?;
            }
        }
        for a in &runtimes {
            report.push(format!(
                "agent {} is up ({}, tools: the gateway's only)",
                a.name,
                a.runtime.as_deref().unwrap_or_default()
            ));
        }
    }

    compose.up(&["open-webui"])?;
    // A removed agent's networks: Compose doesn't remove a network it no
    // longer declares.
    let mut wanted: Vec<String> = vec![];
    for a in &runtimes {
        wanted.extend(crate::hermes::networks(&a.name));
    }
    for n in compose.project_networks()? {
        if n.starts_with("agent-") && !wanted.contains(&n) {
            compose.remove_network_if_unused(&n)?;
        }
    }
    // The pre-008 shared network: Compose doesn't remove a network it no
    // longer declares (specs/008 research.md R4).
    match compose.remove_network_if_unused("default")? {
        NetworkRemoval::Removed => {
            report.push("removed the old shared network sesh_default (specs/008)".into())
        }
        NetworkRemoval::InUse(users) => report.push(format!(
            "warning: sesh_default is still used by {}; `sesh doctor` will fail until they're recreated",
            users.join(", ")
        )),
        NetworkRemoval::Absent => {}
    }
    report.push(format!(
        "stack is up: Open WebUI on http://127.0.0.1:{}, router on {url}, gateway on http://127.0.0.1:7000/mcp ({} agents)",
        config.ports.webui,
        agents.agents.len()
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

fn delete_client_key(http: &dyn Http, url: &str, master: &str, key: &str) -> Result<()> {
    let r = http.post(
        &format!("{url}/key/delete"),
        Some(master),
        &json!({"keys": [key]}),
    )?;
    // Already gone is fine: the key no longer works either way.
    if r.status != 200 && r.status != 404 {
        bail!("router refused to delete a key ({}): {}", r.status, r.body);
    }
    Ok(())
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

    fn passwords() -> Vec<(&'static str, String)> {
        SERVICE_DATABASES
            .iter()
            .enumerate()
            .map(|(i, (role, _))| (*role, format!("pw-{role}-{i}")))
            .collect()
    }

    #[test]
    fn only_the_stub_lets_miniflux_fetch_private_addresses() {
        // ADR 0009: the household stack keeps Miniflux's default refusal
        // of private, loopback and link-local addresses.
        assert!(!super::COMPOSE_YAML.contains("FETCHER_ALLOW_PRIVATE_NETWORKS"));
        assert!(super::COMPOSE_STUB_YAML.contains("FETCHER_ALLOW_PRIVATE_NETWORKS"));
    }

    #[test]
    fn the_migration_gives_every_service_its_own_role_and_passes_passwords_as_variables() {
        let pws = passwords();
        let refs: Vec<(&str, &str)> = pws.iter().map(|(r, p)| (*r, p.as_str())).collect();
        let sql = migration_sql("gw-pw-1", &refs).unwrap();
        let (head, rest) = sql
            .split_once(GATEWAY_SQL)
            .expect("the migration follows the variables");
        assert!(head.contains("\\set gateway_pw 'gw-pw-1'\n"), "{head}");
        assert_eq!(sql.matches("gw-pw-1").count(), 1);
        for (role, pw) in &refs {
            assert!(
                head.contains(&format!("\\set {role}_pw '{pw}'\n")),
                "{head}"
            );
            assert_eq!(
                sql.matches(pw).count(),
                1,
                "{role}'s password appears only in its \\set line"
            );
            assert!(
                rest.contains(&format!("\\c {role}\n{OWNERS_SQL}")),
                "specs/007 R2: the hand-over runs in {role}'s database"
            );
        }
        let m = GATEWAY_SQL;
        for (role, _) in SERVICE_DATABASES {
            for needle in [
                format!("CREATE ROLE {role} LOGIN;"),
                format!(
                    "ALTER ROLE {role} LOGIN PASSWORD :'{role}_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;"
                ),
                format!(
                    "SELECT 'CREATE DATABASE {role} OWNER {role}' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '{role}') \\gexec"
                ),
                format!("ALTER DATABASE {role} OWNER TO {role};"),
                format!("REVOKE CONNECT ON DATABASE {role} FROM PUBLIC;"),
                format!("GRANT CONNECT ON DATABASE {role} TO {role};"),
            ] {
                assert!(m.contains(&needle), "specs/007 R1: {needle}");
            }
        }
        // The roles exist before their databases are created or handed over.
        assert!(
            m.find("CREATE ROLE openwebui").unwrap() < m.find("CREATE DATABASE openwebui").unwrap()
        );
        assert!(
            !m.contains("pw-") && !m.contains("gw-pw-1"),
            "no password in the file"
        );
        assert!(
            !m.contains("REASSIGN OWNED") && !OWNERS_SQL.contains("REASSIGN OWNED"),
            "R2: never REASSIGN OWNED"
        );
        // FR-009: the gateway's login keeps exactly its rights.
        let grants: Vec<&str> = m
            .lines()
            .filter(|l| l.contains("TO sesh_gateway"))
            .collect();
        assert_eq!(
            grants,
            vec![
                "GRANT CONNECT ON DATABASE gateway TO sesh_gateway;",
                "GRANT INSERT ON audit TO sesh_gateway;",
                "GRANT USAGE ON SEQUENCE audit_id_seq TO sesh_gateway;",
                "GRANT INSERT, SELECT, UPDATE ON approvals, credential_forms TO sesh_gateway;",
            ]
        );
    }

    #[test]
    fn the_hand_over_covers_every_kind_of_object_in_its_database_only() {
        let o = OWNERS_SQL;
        for needle in [
            "current_database()",
            "rolsuper",
            "SET lock_timeout",
            "ALTER SCHEMA %I OWNER TO %I",
            "'TABLE'",
            "'VIEW'",
            "'MATERIALIZED VIEW'",
            "'FOREIGN TABLE'",
            "ALTER SEQUENCE %s OWNER TO %I",
            "ALTER ROUTINE %s OWNER TO %I",
            "'TYPE'",
            "'DOMAIN'",
            // Extension members and linked sequences are left alone.
            "d.deptype = 'e'",
            "d.deptype IN ('a', 'i') AND d.refclassid = 'pg_class'::regclass",
        ] {
            assert!(o.contains(needle), "specs/007 R2: {needle}");
        }
        assert!(!o.contains("\\c "), "it never changes database itself");
        assert!(!o.contains(":'"), "it takes no variables");
    }

    #[test]
    fn unsafe_passwords_are_refused() {
        for bad in ["a'b", "a\\b", "a\nb", ""] {
            assert!(
                migration_sql("ok", &[("openwebui", bad)]).is_err(),
                "{bad:?}"
            );
            assert!(migration_sql(bad, &[]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_service_password_is_generated() {
        for (_, key) in SERVICE_DATABASES {
            assert!(secrets::GENERATED.contains(&key), "{key}");
        }
        for key in [
            "REDIS_OPENWEBUI_PASSWORD",
            "REDIS_INFISICAL_PASSWORD",
            "REDIS_PAPERLESS_PASSWORD",
        ] {
            assert!(secrets::GENERATED.contains(&key), "{key}");
        }
        assert!(
            !secrets::GENERATED.contains(&"REDIS_PASSWORD"),
            "the shared Redis is gone"
        );
    }

    /// The text of one top-level service (or network) in the Compose file.
    fn block<'a>(y: &'a str, name: &str) -> &'a str {
        let header = format!("\n  {name}:\n");
        let start = y.find(&header).unwrap_or_else(|| panic!("no {name}")) + header.len();
        let rest = &y[start..];
        let mut end = rest.len();
        let mut at = 0;
        for line in rest.split_inclusive('\n') {
            let top = !line.trim().is_empty() && !line.starts_with("   ");
            if top {
                end = at;
                break;
            }
            at += line.len();
        }
        &rest[..end]
    }

    fn members(y: &str, network: &str) -> Vec<String> {
        let services = &y[y.find("\nservices:\n").unwrap()..y.find("\nvolumes:\n").unwrap()];
        let mut out = vec![];
        let mut current = String::new();
        for line in services.lines() {
            if let Some(name) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'))
                && !name.starts_with(' ')
            {
                current = name.to_string();
            }
            if let Some(list) = line.trim().strip_prefix("networks: [")
                && list.trim_end_matches(']').split(", ").any(|n| n == network)
            {
                out.push(current.clone());
            }
        }
        out
    }

    /// Each service's `networks:` list in a Compose text (`["default"]`
    /// when it has none), and each top-level network's `internal` flag.
    fn compose_layout(
        y: &str,
    ) -> (
        std::collections::BTreeMap<String, Vec<String>>,
        std::collections::BTreeMap<String, bool>,
    ) {
        let svc_start = y.find("\nservices:\n").map(|i| i + 1).unwrap_or(0);
        let svc_end = ["\nvolumes:\n", "\nnetworks:\n"]
            .iter()
            .filter_map(|m| y[svc_start..].find(m).map(|i| svc_start + i))
            .min()
            .unwrap_or(y.len());
        let mut services = std::collections::BTreeMap::new();
        let mut current: Option<String> = None;
        // Inside a map-form `networks:` block: its entries are 6-space keys.
        let mut in_map = false;
        for line in y[svc_start..svc_end].lines() {
            if let Some(name) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'))
                && !name.starts_with(' ')
                && !name.starts_with('#')
            {
                current = Some(name.to_string());
                services.insert(name.to_string(), vec!["default".to_string()]);
                in_map = false;
            }
            if line == "    networks:"
                && let Some(c) = &current
            {
                services.insert(c.clone(), vec![]);
                in_map = true;
                continue;
            }
            if in_map {
                if let Some(rest) = line.strip_prefix("      ")
                    && !rest.starts_with(' ')
                    && let Some(n) = rest.split(':').next()
                    && let Some(c) = &current
                {
                    services
                        .get_mut(c)
                        .expect("service")
                        .push(n.trim().to_string());
                    continue;
                }
                if !line.starts_with("       ") {
                    in_map = false;
                }
            }
            if let (Some(c), Some(list)) = (&current, line.trim().strip_prefix("networks: ["))
                && line.starts_with("    networks:")
            {
                let nets: Vec<String> = list
                    .trim_end_matches(']')
                    .split(", ")
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect();
                services.insert(c.clone(), nets);
            }
        }
        let mut networks = std::collections::BTreeMap::new();
        if let Some(i) = y.find("\nnetworks:\n") {
            let mut name: Option<String> = None;
            for line in y[i + "\nnetworks:\n".len()..].lines() {
                if !line.starts_with(' ') && !line.trim().is_empty() {
                    break;
                }
                if let Some(n) = line.strip_prefix("  ").and_then(|l| l.split(':').next())
                    && !line.starts_with("   ")
                    && !n.starts_with('#')
                    && !n.trim().is_empty()
                {
                    name = Some(n.to_string());
                    networks.insert(n.to_string(), false);
                }
                if line.trim() == "internal: true"
                    && let Some(n) = &name
                {
                    networks.insert(n.clone(), true);
                }
            }
        }
        (services, networks)
    }

    #[test]
    fn the_compose_files_match_the_layout_table() {
        use crate::layout;
        let (svc, nets) = compose_layout(COMPOSE_YAML);
        for s in layout::services(false) {
            let mut want = layout::networks_of(s, false);
            want.sort();
            let mut got: Vec<&str> = svc
                .get(s)
                .unwrap_or_else(|| panic!("{s} isn't in compose.yaml"))
                .iter()
                .map(String::as_str)
                .collect();
            got.sort();
            assert_eq!(got, want, "specs/008: {s}'s networks in compose.yaml");
        }
        for (s, n) in &svc {
            assert!(
                !n.iter().any(|x| x == "default"),
                "specs/008: {s} is on default"
            );
        }
        for n in layout::NETWORKS.iter().filter(|n| !n.stub_only) {
            assert_eq!(
                nets.get(n.name),
                Some(&n.internal),
                "network {} (internal: {})",
                n.name,
                n.internal
            );
        }
        assert!(
            !nets.contains_key("default") && !nets.contains_key("stub"),
            "{nets:?}"
        );
        // The stub overlay: `stub` is defined there, and only there.
        let (ssvc, snets) = compose_layout(COMPOSE_STUB_YAML);
        assert_eq!(
            snets.get("stub"),
            Some(&true),
            "stub is internal, in the stub file"
        );
        for s in ["stub-llm", "litellm", "gateway"] {
            let mut want = layout::networks_of(s, true);
            want.sort();
            if let Some(got) = ssvc.get(s).filter(|g| g.as_slice() != ["default"]) {
                let mut got: Vec<&str> = got.iter().map(String::as_str).collect();
                got.sort();
                assert_eq!(got, want, "specs/008: {s}'s networks in stub mode");
            } else {
                panic!("specs/008: compose.stub.yaml must list {s}'s networks");
            }
        }
    }

    #[test]
    fn every_service_has_its_own_login_and_its_own_redis() {
        let y = COMPOSE_YAML;
        assert!(
            !y.contains("://sesh:") && !y.contains("DBUSER: sesh"),
            "specs/007 FR-001: no service logs in as the superuser"
        );
        for (service, url) in [
            (
                "litellm",
                "postgresql://litellm:${LITELLM_DB_PASSWORD:?run sesh init}@postgres:5432/litellm",
            ),
            (
                "open-webui",
                "postgresql://openwebui:${OPENWEBUI_DB_PASSWORD:?run sesh init}@postgres:5432/openwebui",
            ),
            (
                "infisical",
                "postgresql://infisical:${INFISICAL_DB_PASSWORD:?run sesh init}@postgres:5432/infisical",
            ),
            (
                "miniflux",
                "postgres://miniflux:${MINIFLUX_DB_PASSWORD:?run sesh init}@postgres:5432/miniflux",
            ),
        ] {
            assert!(block(y, service).contains(url), "{service}: {url}");
        }
        let p = block(y, "paperless");
        assert!(p.contains("PAPERLESS_DBUSER: paperless\n"));
        assert!(p.contains("PAPERLESS_DBPASS: ${PAPERLESS_DB_PASSWORD:?run sesh init}\n"));
        assert!(!y.contains("\n  redis:\n"), "the shared Redis is gone");
        assert!(!y.contains("@redis:"));
        for (client, redis, network, key) in [
            (
                "open-webui",
                "redis-openwebui",
                "openwebui-redis",
                "REDIS_OPENWEBUI_PASSWORD",
            ),
            (
                "infisical",
                "redis-infisical",
                "infisical-redis",
                "REDIS_INFISICAL_PASSWORD",
            ),
            (
                "paperless",
                "redis-paperless",
                "paperless-redis",
                "REDIS_PAPERLESS_PASSWORD",
            ),
        ] {
            assert!(
                block(y, client).contains(&format!(
                    "redis://:${{{key}:?run sesh init}}@{redis}:6379/0\n"
                )),
                "{client} uses {redis}"
            );
            let r = block(y, redis);
            assert!(r.contains(&format!("REDIS_PASSWORD: ${{{key}:?run sesh init}}\n")));
            assert!(r.contains(&format!("networks: [{network}]\n")), "{redis}");
            assert!(r.contains(&format!("{redis}-data:/data")));
            assert_eq!(
                block(y, network).trim(),
                "internal: true",
                "{network} is internal"
            );
            let mut m = members(y, network);
            m.sort();
            let mut want = vec![client.to_string(), redis.to_string()];
            want.sort();
            assert_eq!(
                m, want,
                "FR-008: only {client} and {redis} are on {network}"
            );
            assert!(
                block(y, client).contains(&format!(
                    "      {redis}:\n        condition: service_healthy"
                )),
                "{client} waits for {redis}"
            );
        }
    }
    use crate::host::fake::{FakeCompose, FakeHttp};
    use crate::init::{self, InitOptions};

    fn ready(name: &str) -> PathBuf {
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
            if let Some(r) = crate::infisical::tests::reply(url) {
                return r;
            }
            if url.ends_with("/key/generate") {
                assert_eq!(body.unwrap()["models"], json!(["agent-main"]));
                (200, json!({"key": "sk-client"}))
            } else {
                (200, json!({}))
            }
        })
    }

    /// A fake with an address range for every database network.
    fn fake() -> FakeCompose {
        let f = FakeCompose::default();
        for (i, (_, _, net)) in crate::layout::DB_LOGINS.iter().enumerate() {
            f.subnets
                .borrow_mut()
                .insert(net.to_string(), vec![format!("172.{}.0.0/16", 40 + i)]);
        }
        f
    }

    #[test]
    fn up_binds_each_database_login_to_its_network_before_services_start() {
        let dir = ready("hba");
        let compose = fake();
        let report = run(&dir, &compose, &router()).unwrap();
        let calls = compose.calls.borrow().clone();
        let at = |c: &str| {
            calls
                .iter()
                .position(|x| x == c)
                .unwrap_or_else(|| panic!("{c}: {calls:?}"))
        };
        assert!(at("sql sesh") < at("reload postgres"), "{calls:?}");
        assert!(
            at("reload postgres")
                < calls
                    .iter()
                    .position(|c| c.starts_with("up redis"))
                    .unwrap()
        );
        let rules = std::fs::read_to_string(dir.join(crate::pg_hba::DIR).join(crate::pg_hba::FILE))
            .unwrap();
        assert!(
            rules.contains("host litellm litellm 172.40.0.0/16 scram-sha-256"),
            "{rules}"
        );
        assert!(rules.trim_end().ends_with("host all all all reject"));
        assert!(
            report
                .iter()
                .any(|l| l.contains("database login rules updated")),
            "{report:?}"
        );
        // Unchanged ranges: no rewrite, no reload, no message.
        let again = fake();
        let report = run(&dir, &again, &router()).unwrap();
        assert!(!again.calls.borrow().iter().any(|c| c == "reload postgres"));
        assert!(!report.iter().any(|l| l.contains("database login rules")));
    }

    #[test]
    fn up_removes_the_old_shared_network_once_unused() {
        let dir = ready("old-net");
        let compose = fake();
        compose
            .networks
            .borrow_mut()
            .insert("default".into(), vec![]);
        let report = run(&dir, &compose, &router()).unwrap();
        assert!(
            compose
                .calls
                .borrow()
                .iter()
                .any(|c| c == "rm network default")
        );
        assert!(
            report
                .iter()
                .any(|l| l.contains("removed the old shared network sesh_default")),
            "{report:?}"
        );
        // Still in use: a warning naming the containers, not a failure.
        let busy = fake();
        busy.networks
            .borrow_mut()
            .insert("default".into(), vec!["sesh-ntfy-1".into()]);
        let report = run(&dir, &busy, &router()).unwrap();
        assert!(
            report
                .iter()
                .any(|l| l.contains("sesh_default is still used by sesh-ntfy-1")),
            "{report:?}"
        );
        // Absent: nothing said.
        let report = run(&dir, &fake(), &router()).unwrap();
        assert!(!report.iter().any(|l| l.contains("sesh_default")));
    }

    #[test]
    fn an_unreadable_network_range_fails_up_before_any_service_starts() {
        let dir = ready("hba-missing");
        let compose = fake();
        compose.subnets.borrow_mut().remove("paperless-db");
        let e = run(&dir, &compose, &router()).unwrap_err().to_string();
        assert!(
            e.contains("sesh_paperless-db") && e.contains("stay refused"),
            "{e}"
        );
        assert!(
            !compose
                .calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("up redis")),
            "fails closed"
        );
        let rules = std::fs::read_to_string(dir.join(crate::pg_hba::DIR).join(crate::pg_hba::FILE))
            .unwrap();
        assert!(
            !rules.contains("host paperless"),
            "no rule was widened: {rules}"
        );
    }

    #[test]
    fn compose_files_start_postgres_with_local_only_rules() {
        let dir = ready("hba-initial");
        let config = Config::load(&dir).unwrap();
        write_compose_files(&dir, &config).unwrap();
        let rules = std::fs::read_to_string(dir.join(crate::pg_hba::DIR).join(crate::pg_hba::FILE))
            .unwrap();
        assert_eq!(rules, crate::pg_hba::initial());
        let pg = block(COMPOSE_YAML, "postgres");
        assert!(pg.contains("hba_file=/etc/sesh-pg/pg_hba.conf"), "{pg}");
        assert!(
            pg.contains("/postgres-conf:/etc/sesh-pg:ro"),
            "a read-only directory mount: {pg}"
        );
    }

    #[test]
    fn first_up_migrates_then_starts_services_and_webui_last() {
        let dir = ready("first");
        crate::agent::add(&dir, "alice", &["search".into()], "household", None).unwrap();
        let compose = fake();
        run(&dir, &compose, &router()).unwrap();
        assert_eq!(
            *compose.calls.borrow(),
            vec![
                "up postgres",
                "sql sesh",
                "reload postgres",
                "up redis-openwebui redis-infisical redis-paperless litellm searxng infisical ntfy paperless miniflux gateway executor-alice stub-llm",
                "restart gateway",
                "up open-webui"
            ]
        );
        assert_eq!(
            Secrets::load(&dir).unwrap().get("WEBUI_ROUTER_KEY"),
            Some("sk-client")
        );
        // The eval's own alias-only key (specs/003 R9).
        assert_eq!(
            Secrets::load(&dir).unwrap().get("EVAL_ROUTER_KEY"),
            Some("sk-client")
        );
        let g = std::fs::read_to_string(dir.join("gateway/gateway.toml")).unwrap();
        assert!(
            g.contains("http://stub-llm:8000"),
            "stub mode points upstreams at the fakes"
        );
    }

    #[test]
    fn a_runtime_agent_is_started_after_its_keys_and_removed_cleanly() {
        let dir = ready("runtime");
        crate::agent::add(
            &dir,
            "house",
            &["search".into()],
            "household",
            Some("hermes"),
        )
        .unwrap();
        let compose = fake();
        let http = router();
        let report = run(&dir, &compose, &http).unwrap();
        let calls = compose.calls.borrow().clone();
        let at = |c: &str| {
            calls
                .iter()
                .position(|x| x == c)
                .unwrap_or_else(|| panic!("{c}: {calls:?}"))
        };
        assert!(at("up hermes-house") < at("up open-webui"), "{calls:?}");
        assert!(
            report
                .iter()
                .any(|l| l == "created house's router key (limited to the aliases)"),
            "{report:?}"
        );
        assert!(
            report
                .iter()
                .any(|l| l == "agent house is up (hermes, tools: the gateway's only)")
        );
        let s = Secrets::load(&dir).unwrap();
        assert_eq!(s.get("AGENT_HOUSE_ROUTER_KEY"), Some("sk-client"));
        let env = std::fs::read_to_string(crate::hermes::env_path(&dir, "house")).unwrap();
        assert!(
            env.contains("SESH_ROUTER_KEY=sk-client\n"),
            "the env file has the new key"
        );
        let token = std::fs::read_to_string(crate::agent::token_path(&dir, "house")).unwrap();
        assert!(env.contains(&format!("SESH_GATEWAY_TOKEN={}\n", token.trim())));
        assert!(env.contains(&format!(
            "API_SERVER_KEY={}\n",
            s.get("AGENT_HOUSE_API_KEY").unwrap()
        )));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(crate::hermes::env_path(&dir, "house"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(m & 0o777, 0o600);
        }
        let cfg = std::fs::read_to_string(crate::hermes::config_path(&dir, "house")).unwrap();
        for v in [
            token.trim(),
            "sk-client",
            s.get("AGENT_HOUSE_API_KEY").unwrap(),
        ] {
            assert!(!cfg.contains(v), "no secret value in config.yaml");
            assert!(!report.iter().any(|l| l.contains(v)), "never printed");
        }
        let g = std::fs::read_to_string(dir.join("gateway/gateway.toml")).unwrap();
        assert!(
            g.contains("agent_listen = [\"gateway-agent-house\"]"),
            "{g}"
        );
        let overlay = std::fs::read_to_string(dir.join("compose/generated.yaml")).unwrap();
        assert!(!overlay.contains(token.trim()) && !overlay.contains("sk-client"));

        // Remove: the key is deleted in the router, the networks go.
        crate::agent::remove(&dir, "house").unwrap();
        let compose = fake();
        for n in crate::hermes::networks("house") {
            compose.networks.borrow_mut().insert(n, vec![]);
        }
        let http = router();
        let report = run(&dir, &compose, &http).unwrap();
        assert!(
            report.iter().any(|l| l == "removed house's router key"),
            "{report:?}"
        );
        assert!(
            http.calls
                .borrow()
                .iter()
                .any(|c| c.ends_with("/key/delete")),
            "{:?}",
            http.calls.borrow()
        );
        assert!(
            Secrets::load(&dir)
                .unwrap()
                .get("AGENT_HOUSE_ROUTER_KEY")
                .is_none()
        );
        for n in crate::hermes::networks("house") {
            assert!(
                compose.calls.borrow().contains(&format!("rm network {n}")),
                "{n}"
            );
        }
        assert!(
            !compose
                .calls
                .borrow()
                .iter()
                .any(|c| c.contains("hermes-house"))
        );
        assert!(!crate::hermes::env_path(&dir, "house").exists());
    }

    #[test]
    fn up_generates_the_passwords_an_older_install_lacks() {
        let dir = ready("upgrade");
        let mut s = Secrets::load(&dir).unwrap();
        for k in ["OPENWEBUI_DB_PASSWORD", "REDIS_PAPERLESS_PASSWORD"] {
            s.remove(k);
        }
        s.save(&dir).unwrap();
        let compose = fake();
        let report = run(&dir, &compose, &router()).unwrap();
        for k in ["OPENWEBUI_DB_PASSWORD", "REDIS_PAPERLESS_PASSWORD"] {
            assert!(
                report.contains(&format!("generated {k}")),
                "specs/007 FR-007: {report:?}"
            );
        }
        let s = Secrets::load(&dir).unwrap();
        let pw = s.get("OPENWEBUI_DB_PASSWORD").expect("saved");
        assert!(!report.iter().any(|l| l.contains(pw)), "never printed");
        assert_eq!(compose.calls.borrow()[1], "sql sesh", "before services");
        // Nothing missing: nothing generated.
        let report = run(&dir, &fake(), &router()).unwrap();
        assert!(!report.iter().any(|l| l.starts_with("generated")));
    }

    #[test]
    fn later_up_restarts_only_what_changed() {
        let dir = ready("later");
        run(&dir, &fake(), &router()).unwrap();
        let compose = fake();
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
        let compose = fake();
        run(&dir, &compose, &router()).unwrap();
        let calls = compose.calls.borrow();
        assert!(calls.contains(&"restart litellm".to_string()));
        // Leaving stub mode changes the gateway's upstreams too.
        assert!(calls.contains(&"restart gateway".to_string()));
    }

    #[test]
    fn overlay_isolates_each_executor() {
        let mut f = AgentsFile::default();
        for n in ["alice", "bob"] {
            f.agents.push(sesh_core::Agent {
                owner: sesh_core::HOUSEHOLD.into(),
                name: n.into(),
                token_sha256: "0".repeat(64),
                connectors: vec!["search".into()],
                runtime: None,
            });
        }
        let y = generated_overlay(&f, false);
        assert!(y.contains("executor-alice:") && y.contains("networks: [exec-alice]"));
        let gw = block(&y, "gateway");
        assert!(
            gw.contains("      exec-alice: {}\n      exec-bob: {}\n"),
            "{gw}"
        );
        // The healthcheck asks workerd itself, not just the supervisor's port.
        assert!(y.contains("/usr/local/bin/sesh-executor-supervisor\", \"health\""));
        assert!(!y.contains("/dev/tcp"));
        assert!(y.contains("exec-bob:\n    internal: true"));
        assert!(!y.contains("kiwix"));
        assert!(generated_overlay(&f, true).contains("kiwix-serve:3.8.2@sha256:"));
        assert!(generated_overlay(&AgentsFile::default(), false).contains("services: {}"));
    }

    fn runtime_agents() -> AgentsFile {
        let mut f = AgentsFile::default();
        for (n, rt) in [("house", Some("hermes")), ("bob", None)] {
            f.agents.push(sesh_core::Agent {
                owner: sesh_core::HOUSEHOLD.into(),
                name: n.into(),
                token_sha256: "0".repeat(64),
                connectors: vec!["search".into()],
                runtime: rt.map(String::from),
            });
        }
        f
    }

    #[test]
    fn a_runtime_agent_gets_its_container_and_is_reached_through_the_gateway() {
        let y = generated_overlay(&runtime_agents(), false);
        let h = block(&y, "hermes-house");
        assert!(
            h.contains("networks: [agent-house, agent-house-model]"),
            "{h}"
        );
        assert!(
            !y.contains("hermes-bob"),
            "an identity-only agent gets no runtime"
        );
        // The gateway is on the agent's own network and on `chat`, with its
        // aliases there (specs/009 R5, specs/010 R2).
        let gw = block(&y, "gateway");
        assert!(
            gw.contains("      agent-house:\n        aliases: [gateway-agent-house]\n"),
            "{gw}"
        );
        assert!(
            gw.contains("      chat:\n        aliases: [gateway-chat]\n"),
            "{gw}"
        );
        assert!(!gw.contains("agent-house-model"));
        assert!(block(&y, "litellm").contains("networks: [agent-house-model]"));
        let ow = block(&y, "open-webui");
        assert!(
            !ow.contains("networks:"),
            "Open WebUI reaches no runtime directly: {ow}"
        );
        assert!(
            ow.contains("OPENAI_API_BASE_URLS: \"http://litellm:4000/v1;http://gateway-chat:7003/agent/house/v1\""),
            "{ow}"
        );
        assert!(
            ow.contains(
                "OPENAI_API_KEYS: \"${WEBUI_ROUTER_KEY:-pending};${AGENT_HOUSE_API_KEY:-pending}\""
            ),
            "{ow}"
        );
        assert!(ow.contains("ENABLE_FORWARD_USER_INFO_HEADERS: \"true\""));
        assert!(
            ow.contains("FORWARD_USER_INFO_HEADER_JWT_SECRET: ${WEBUI_JWT_SECRET:?run sesh up}")
        );
        for n in ["agent-house", "agent-house-model"] {
            assert!(
                y.contains(&format!("\n  {n}:\n    internal: true\n")),
                "{n}"
            );
        }
        assert!(!y.contains("agent-house-chat"));
        assert!(y.contains("\nvolumes:\n  hermes-house-data: {}\n"));
        // Without runtimes, Open WebUI and LiteLLM aren't touched.
        let mut f = runtime_agents();
        f.agents[0].runtime = None;
        let y = generated_overlay(&f, false);
        assert!(!y.contains("open-webui:") && !y.contains("litellm:") && !y.contains("volumes:"));
    }

    #[test]
    fn an_agent_with_the_browser_gets_a_hardened_browser_on_its_own_network() {
        let mut f = runtime_agents();
        f.agents[1].connectors.push("browser".into());
        let y = generated_overlay(&f, false);
        let b = block(&y, "browser-bob");
        for want in [
            "networks: [browser-bob]",
            "read_only: true",
            "cap_drop: [ALL]",
            "no-new-privileges:true",
            "SESH_PROXY: http://gateway-browser-bob:7004",
            "browser-bob-profile:/profile",
        ] {
            assert!(b.contains(want), "{want}: {b}");
        }
        assert!(!b.contains("ports:"));
        assert!(y.contains("\n  browser-bob:\n    internal: true\n"));
        let gw = block(&y, "gateway");
        assert!(
            gw.contains("      browser-bob:\n        aliases: [gateway-browser-bob]\n"),
            "{gw}"
        );
        assert!(
            !y.contains("browser-house:"),
            "only agents granted the browser"
        );
    }

    #[test]
    fn the_gateway_listens_on_each_runtimes_alias_only() {
        assert_eq!(
            agent_listen(&runtime_agents()),
            vec!["gateway-agent-house".to_string()]
        );
    }
}
