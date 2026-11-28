//! Fakes for the seams, used by unit tests.

use super::compose::{Compose, NetworkRemoval, Published, ServiceState};
use super::http::{Http, Reply};
use super::tailscale::Tailscale;
use anyhow::Result;
use serde_json::Value;
use std::cell::RefCell;

#[derive(Default)]
pub struct FakeCompose {
    pub services: RefCell<Vec<ServiceState>>,
    pub calls: RefCell<Vec<String>>,
    /// What `exec_sql` returns.
    pub sql_output: RefCell<String>,
    /// Each network's subnets; a network that's absent has none.
    pub subnets: RefCell<std::collections::BTreeMap<String, Vec<String>>>,
    /// Networks that exist, with the containers attached.
    pub networks: RefCell<std::collections::BTreeMap<String, Vec<String>>>,
    /// Each running service's networks.
    pub service_nets: RefCell<Vec<(String, Vec<String>)>>,
    /// `exec_sql` answers by query: the first (needle, output) whose needle
    /// the SQL contains wins; otherwise `sql_output`.
    pub sql_by_query: RefCell<Vec<(String, String)>>,
    /// `exec` answers: (service, output); a service without one fails.
    pub exec_output: RefCell<Vec<(String, String)>>,
}

impl FakeCompose {
    pub fn healthy(services: &[(&str, Option<(&str, u16)>)]) -> Self {
        let states = services
            .iter()
            .map(|(name, publish)| ServiceState {
                service: name.to_string(),
                state: "running".into(),
                health: "healthy".into(),
                published: publish
                    .map(|(addr, port)| {
                        vec![Published {
                            address: addr.to_string(),
                            host_port: port,
                            target_port: port,
                        }]
                    })
                    .unwrap_or_default(),
            })
            .collect();
        FakeCompose {
            services: RefCell::new(states),
            ..Default::default()
        }
    }
}

impl Compose for FakeCompose {
    fn up(&self, services: &[&str]) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("up {}", services.join(" ")));
        Ok(())
    }
    fn restart(&self, service: &str) -> Result<()> {
        self.calls.borrow_mut().push(format!("restart {service}"));
        Ok(())
    }
    fn down(&self, remove_volumes: bool) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("down {remove_volumes}"));
        Ok(())
    }
    fn ps(&self) -> Result<Vec<ServiceState>> {
        Ok(self.services.borrow().clone())
    }
    fn exec_sql(&self, database: &str, sql: &str) -> Result<String> {
        if sql.trim() == "SELECT pg_reload_conf();" {
            self.calls.borrow_mut().push("reload postgres".into());
            return Ok("t\n".into());
        }
        self.calls.borrow_mut().push(format!("sql {database}"));
        if let Some((_, out)) = self
            .sql_by_query
            .borrow()
            .iter()
            .find(|(n, _)| sql.contains(n.as_str()))
        {
            return Ok(out.clone());
        }
        Ok(self.sql_output.borrow().clone())
    }
    fn network_subnets(&self, network: &str) -> Result<Vec<String>> {
        Ok(self
            .subnets
            .borrow()
            .get(network)
            .cloned()
            .unwrap_or_default())
    }
    fn remove_network_if_unused(&self, network: &str) -> Result<NetworkRemoval> {
        let mut nets = self.networks.borrow_mut();
        match nets.get(network) {
            None => Ok(NetworkRemoval::Absent),
            Some(users) if !users.is_empty() => Ok(NetworkRemoval::InUse(users.clone())),
            Some(_) => {
                nets.remove(network);
                self.calls
                    .borrow_mut()
                    .push(format!("rm network {network}"));
                Ok(NetworkRemoval::Removed)
            }
        }
    }
    fn service_networks(&self) -> Result<Vec<(String, Vec<String>)>> {
        Ok(self.service_nets.borrow().clone())
    }
    fn project_networks(&self) -> Result<Vec<String>> {
        Ok(self.networks.borrow().keys().cloned().collect())
    }
    fn exec(&self, service: &str, args: &[&str]) -> Result<String> {
        self.calls
            .borrow_mut()
            .push(format!("exec {service} {}", args.join(" ")));
        self.exec_output
            .borrow()
            .iter()
            .find(|(s, _)| s == service)
            .map(|(_, o)| o.clone())
            .ok_or_else(|| anyhow::anyhow!("{service} isn't running"))
    }
}

type Responder = Box<dyn Fn(&str, &str, Option<&Value>) -> (u16, Value)>;

/// Answers each request with a closure of (method, url, body).
pub struct FakeHttp {
    pub respond: Responder,
    pub calls: RefCell<Vec<String>>,
}

impl FakeHttp {
    pub fn new(f: impl Fn(&str, &str, Option<&Value>) -> (u16, Value) + 'static) -> Self {
        FakeHttp {
            respond: Box::new(f),
            calls: RefCell::default(),
        }
    }
}

impl Http for FakeHttp {
    fn get(&self, url: &str, _bearer: Option<&str>) -> Result<Reply> {
        self.calls.borrow_mut().push(format!("GET {url}"));
        let (status, body) = (self.respond)("GET", url, None);
        Ok(Reply { status, body })
    }
    fn post(&self, url: &str, _bearer: Option<&str>, body: &Value) -> Result<Reply> {
        self.calls.borrow_mut().push(format!("POST {url}"));
        let (status, body) = (self.respond)("POST", url, Some(body));
        Ok(Reply { status, body })
    }
}

pub struct FakeTailscale(pub Option<bool>);

impl Tailscale for FakeTailscale {
    fn serves_port(&self, _port: u16) -> Result<Option<bool>> {
        Ok(self.0)
    }
}

type ToolAnswer = Box<dyn Fn(&str, &Value) -> super::mcp::ToolResult>;

/// A gateway that answers `tools/call` with a closure of (tool, args).
pub struct FakeMcp {
    pub tools: Vec<Value>,
    pub calls: RefCell<Vec<(String, Value)>>,
    pub answer: ToolAnswer,
}

impl FakeMcp {
    /// The gateway's three tools, answering every call with `text`.
    pub fn answering(text: &'static str) -> Self {
        let tool = |n: &str| serde_json::json!({"name": n, "description": n, "inputSchema": {"type": "object"}});
        FakeMcp {
            tools: vec![tool("execute"), tool("search"), tool("describe")],
            calls: RefCell::default(),
            answer: Box::new(move |_, _| super::mcp::ToolResult {
                is_error: false,
                text: text.to_string(),
            }),
        }
    }
}

impl super::mcp::Mcp for FakeMcp {
    fn list_tools(&self) -> Result<(Vec<Value>, String)> {
        Ok((self.tools.clone(), "use the tools".into()))
    }

    fn call_tool(&self, name: &str, args: &Value) -> Result<super::mcp::ToolResult> {
        self.calls
            .borrow_mut()
            .push((name.to_string(), args.clone()));
        Ok((self.answer)(name, args))
    }
}
