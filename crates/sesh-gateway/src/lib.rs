//! sesh-gateway: the only way out for agents (constitution II, ADR 0004).
//!
//! Two listeners: agents reach the MCP tools on `:7000` with their token;
//! executors reach `/call` on `:7001` with a per-execution capability.

#![forbid(unsafe_code)]

pub mod approvals;
pub mod audit;
pub mod auth;
pub mod calls;
pub mod capability;
pub mod connectors;
pub mod executor;
pub mod google;
pub mod listen;
pub mod mcp;
pub mod notify;
pub mod owners;
pub mod pages;
pub mod redact;
pub mod refs;
pub mod schema;
pub mod secrets;

use sesh_core::{Agent, AgentsFile, ConnectorDescriptor, GatewaySettings, User, UsersFile};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Reloads `agents.toml` when its modification time changes, so
/// `sesh agent add/remove` takes effect without a restart.
pub struct Agents {
    path: Option<PathBuf>,
    cached: Mutex<(Option<SystemTime>, AgentsFile)>,
}

impl Agents {
    pub fn from_file(path: PathBuf) -> Self {
        Agents {
            path: Some(path),
            cached: Mutex::new((None, AgentsFile::default())),
        }
    }

    pub fn fixed(file: AgentsFile) -> Self {
        Agents {
            path: None,
            cached: Mutex::new((None, file)),
        }
    }

    fn current(&self) -> AgentsFile {
        let mut c = self.cached.lock().expect("lock");
        if let Some(path) = &self.path {
            let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
            if mtime != c.0 {
                match std::fs::read_to_string(path).map(|t| AgentsFile::parse(&t)) {
                    Ok(Ok(f)) => *c = (mtime, f),
                    // An unreadable file means no agents: fail closed.
                    _ => *c = (mtime, AgentsFile::default()),
                }
            }
        }
        c.1.clone()
    }

    pub fn by_token(&self, token: &str) -> Option<Agent> {
        self.current().by_token(token).cloned()
    }

    pub fn get(&self, name: &str) -> Option<Agent> {
        self.current().get(name).cloned()
    }
}

/// Reloads `users.toml` when it changes (specs/005); fails closed.
pub struct Users {
    path: Option<PathBuf>,
    cached: Mutex<(Option<SystemTime>, UsersFile)>,
}

impl Users {
    pub fn from_file(path: PathBuf) -> Self {
        Users {
            path: Some(path),
            cached: Mutex::new((None, UsersFile::default())),
        }
    }

    pub fn fixed(file: UsersFile) -> Self {
        Users {
            path: None,
            cached: Mutex::new((None, file)),
        }
    }

    pub fn get(&self, name: &str) -> Option<User> {
        let mut c = self.cached.lock().expect("lock");
        if let Some(path) = &self.path {
            let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
            if mtime != c.0 {
                match std::fs::read_to_string(path).map(|t| UsersFile::parse(&t)) {
                    Ok(Ok(f)) => *c = (mtime, f),
                    _ => *c = (mtime, UsersFile::default()),
                }
            }
        }
        c.1.get(name).cloned()
    }
}

pub struct Gateway {
    pub agents: Agents,
    pub users: Users,
    pub settings: GatewaySettings,
    pub connectors: Vec<ConnectorDescriptor>,
    pub caps: capability::Capabilities,
    pub audit: audit::Audit,
    pub upstream: connectors::Upstream,
    /// `{agent}` is replaced by the agent's name.
    pub executor_url: String,
    /// Approval requests and credential forms (specs/004).
    pub approvals: approvals::Store,
    pub notifier: notify::Notifier,
    pub secrets: secrets::SecretStore,
    /// The operator's token for `/admin/*` on the approvals listener.
    pub admin_token: Option<String>,
    /// Approvals a running execution is waiting for, by execution id.
    pub pending: Mutex<std::collections::HashMap<String, Vec<approvals::Request>>>,
}

impl Gateway {
    /// The descriptors of the connectors `agent` may use.
    pub fn connectors_for(&self, agent: &Agent) -> Vec<ConnectorDescriptor> {
        self.connectors
            .iter()
            .filter(|c| agent.connectors.contains(&c.name))
            .cloned()
            .collect()
    }

    pub fn deadline(&self) -> Duration {
        Duration::from_secs(self.settings.deadline_secs)
    }

    pub fn executor_url_for(&self, agent: &str) -> String {
        self.executor_url.replace("{agent}", agent)
    }

    pub fn approval_limits(&self) -> approvals::Limits {
        approvals::Limits {
            ttl_secs: self.settings.approval_ttl_secs,
            pending_max: self.settings.approvals_pending_max,
            per_hour: self.settings.approvals_per_hour,
            base_url: self.settings.approvals_base_url.clone(),
        }
    }

    /// Records that `execution` is waiting for `r`.
    pub fn note_pending(&self, execution: &str, r: &approvals::Request) {
        let mut p = self.pending.lock().expect("lock");
        let list = p.entry(execution.to_string()).or_default();
        if !list.iter().any(|x| x.id == r.id) {
            list.push(r.clone());
        }
    }

    pub fn take_pending(&self, execution: &str) -> Vec<approvals::Request> {
        self.pending
            .lock()
            .expect("lock")
            .remove(execution)
            .unwrap_or_default()
    }
}
