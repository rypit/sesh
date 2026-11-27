//! Seam: the only module that talks to a gateway's agent endpoint, as an
//! MCP client over Streamable HTTP (specs/003-model-evals, ADR 0006).
//! Replies are SSE or JSON; the session id comes from the
//! `initialize` reply.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::time::Duration;

/// One `tools/call` result: the text content, and whether it was an error.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub is_error: bool,
    pub text: String,
}

pub trait Mcp {
    /// The server's tools (`name`, `description`, `inputSchema`) and
    /// instructions.
    fn list_tools(&self) -> Result<(Vec<Value>, String)>;
    fn call_tool(&self, name: &str, args: &Value) -> Result<ToolResult>;
}

pub struct HttpMcp {
    url: String,
    token: String,
    agent: ureq::Agent,
    session: RefCell<Option<String>>,
    next_id: Cell<u64>,
    instructions: RefCell<String>,
}

impl HttpMcp {
    /// Connects and initializes; fails with the endpoint named on 401.
    pub fn connect(url: &str, token: &str, timeout: Duration) -> Result<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let m = HttpMcp {
            url: url.to_string(),
            token: token.to_string(),
            agent,
            session: RefCell::new(None),
            next_id: Cell::new(1),
            instructions: RefCell::new(String::new()),
        };
        let init = m.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "sesh-eval", "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        *m.instructions.borrow_mut() = init["instructions"].as_str().unwrap_or("").to_string();
        m.post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(m)
    }

    fn post(&self, body: &Value) -> Result<(u16, Option<Value>)> {
        let mut r = self
            .agent
            .post(&self.url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/json, text/event-stream");
        if let Some(sid) = self.session.borrow().as_ref() {
            r = r.header("Mcp-Session-Id", sid);
        }
        let mut resp = r
            .send_json(body)
            .with_context(|| format!("can't reach the gateway at {}", self.url))?;
        let status = resp.status().as_u16();
        if let Some(sid) = resp.headers().get("mcp-session-id")
            && let Ok(sid) = sid.to_str()
        {
            *self.session.borrow_mut() = Some(sid.to_string());
        }
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        Ok((status, parse_reply(&text)))
    }

    fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let (status, msg) =
            self.post(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        if status == 401 {
            bail!("the gateway at {} refused the eval agent's token", self.url);
        }
        if status != 200 {
            bail!("the gateway at {} answered {status} to {method}", self.url);
        }
        let msg = msg.with_context(|| format!("no reply to {method}"))?;
        if let Some(e) = msg.get("error") {
            bail!(
                "{method} failed: {}",
                e["message"].as_str().unwrap_or("unknown error")
            );
        }
        Ok(msg["result"].clone())
    }
}

/// The last JSON-RPC message in an SSE stream, or a plain JSON body.
pub fn parse_reply(text: &str) -> Option<Value> {
    let data: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .collect();
    data.iter()
        .rev()
        .find_map(|d| serde_json::from_str(d).ok())
        .or_else(|| serde_json::from_str(text).ok())
}

impl Mcp for HttpMcp {
    fn list_tools(&self) -> Result<(Vec<Value>, String)> {
        let r = self.request("tools/list", json!({}))?;
        let tools = r["tools"].as_array().cloned().unwrap_or_default();
        Ok((tools, self.instructions.borrow().clone()))
    }

    fn call_tool(&self, name: &str, args: &Value) -> Result<ToolResult> {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}))?;
        let text = r["content"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(ToolResult {
            is_error: r["isError"].as_bool().unwrap_or(false),
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-endpoint MCP server: SSE replies, a session id, and 401 for a wrong token.
    fn serve() -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mcp", server.server_addr().to_ip().unwrap());
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let auth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string())
                    .unwrap_or_default();
                if auth != "Bearer good" {
                    let _ = req.respond(tiny_http::Response::from_string("").with_status_code(401));
                    continue;
                }
                let has_session = req
                    .headers()
                    .iter()
                    .any(|h| h.field.equiv("Mcp-Session-Id"));
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let msg: Value = serde_json::from_str(&body).unwrap();
                let id = msg["id"].clone();
                let result = match msg["method"].as_str().unwrap() {
                    "initialize" => json!({"instructions": "use execute"}),
                    "tools/list" if has_session => {
                        json!({"tools": [{"name": "execute", "description": "run", "inputSchema": {"type": "object"}}]})
                    }
                    "tools/call" if has_session => {
                        json!({"content": [{"type": "text", "text": format!("ran {}", msg["params"]["arguments"])}], "isError": false})
                    }
                    _ => json!(null),
                };
                let sse = format!(
                    "event: message\ndata: {}\n\n",
                    json!({"jsonrpc": "2.0", "id": id, "result": result})
                );
                let resp = tiny_http::Response::from_string(sse)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"text/event-stream"[..],
                        )
                        .unwrap(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes(&b"Mcp-Session-Id"[..], &b"s1"[..]).unwrap(),
                    );
                let _ = req.respond(resp);
            }
        });
        url
    }

    #[test]
    fn lists_and_calls_tools_over_a_session() {
        let url = serve();
        let m = HttpMcp::connect(&url, "good", Duration::from_secs(5)).unwrap();
        let (tools, instructions) = m.list_tools().unwrap();
        assert_eq!(tools[0]["name"], "execute");
        assert_eq!(instructions, "use execute");
        let r = m.call_tool("execute", &json!({"code": "1"})).unwrap();
        assert!(!r.is_error && r.text.contains("\"code\""), "{r:?}");
    }

    #[test]
    fn a_wrong_token_names_the_endpoint() {
        let url = serve();
        let e = HttpMcp::connect(&url, "bad", Duration::from_secs(5))
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains(&url) && e.contains("refused"), "{e}");
    }

    #[test]
    fn parses_sse_and_json() {
        assert_eq!(
            parse_reply("data: {\"a\":1}\n\ndata: {\"a\":2}\n").unwrap(),
            json!({"a": 2})
        );
        assert_eq!(parse_reply("{\"a\":3}").unwrap(), json!({"a": 3}));
        assert!(parse_reply("").is_none());
    }
}
