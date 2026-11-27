//! Fakes for the seams, used by unit tests.

use super::compose::{Compose, Published, ServiceState};
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
            calls: RefCell::default(),
            sql_output: RefCell::default(),
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
    fn exec_sql(&self, database: &str, _sql: &str) -> Result<String> {
        self.calls.borrow_mut().push(format!("sql {database}"));
        Ok(self.sql_output.borrow().clone())
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
