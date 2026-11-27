//! Seam: HTTP calls to the alias router and Open WebUI.

use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

pub trait Http {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<Reply>;
    fn post(&self, url: &str, bearer: Option<&str>, body: &Value) -> Result<Reply>;
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new(timeout: Duration) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        UreqHttp { agent }
    }
}

fn reply(mut resp: ureq::http::Response<ureq::Body>) -> Result<Reply> {
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
    Ok(Reply { status, body })
}

impl Http for UreqHttp {
    fn get(&self, url: &str, bearer: Option<&str>) -> Result<Reply> {
        let mut req = self.agent.get(url);
        if let Some(key) = bearer {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        reply(req.call()?)
    }

    fn post(&self, url: &str, bearer: Option<&str>, body: &Value) -> Result<Reply> {
        let mut req = self.agent.post(url);
        if let Some(key) = bearer {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        reply(req.send_json(body)?)
    }
}
