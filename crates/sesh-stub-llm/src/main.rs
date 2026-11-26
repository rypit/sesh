//! An OpenAI-compatible stub model server for development and CI.
//!
//! - `GET /v1/models` lists `stub-model` and `stub-raw-2`.
//! - `POST /v1/chat/completions` replies "stub reply", or, when the request
//!   offers tools (and `STUB_TOOLS` isn't `off`), calls the first tool with
//!   `{}`. `"stream": true` returns server-sent events.
//! - `GET /health` returns 200.
//!
//! Listens on `0.0.0.0:$STUB_PORT` (default 8000) inside its container; the
//! Compose overlay publishes no port for it.

use serde_json::{Value, json};

const MODELS: [&str; 2] = ["stub-model", "stub-raw-2"];

fn models() -> Value {
    json!({
        "object": "list",
        "data": MODELS.iter().map(|m| json!({"id": m, "object": "model", "owned_by": "stub"})).collect::<Vec<_>>()
    })
}

fn first_tool(req: &Value, tools_enabled: bool) -> Option<String> {
    if !tools_enabled {
        return None;
    }
    req.get("tools")?
        .as_array()?
        .first()?
        .pointer("/function/name")?
        .as_str()
        .map(str::to_string)
}

/// A non-streamed completion.
fn completion(req: &Value, tools_enabled: bool) -> Value {
    let model = req
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("stub-model");
    let (message, finish) = match first_tool(req, tools_enabled) {
        Some(name) => (
            json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_stub_1", "type": "function", "function": {"name": name, "arguments": "{}"}}
            ]}),
            "tool_calls",
        ),
        None => (
            json!({"role": "assistant", "content": "stub reply"}),
            "stop",
        ),
    };
    json!({
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
    })
}

/// The same completion as server-sent events.
fn stream(req: &Value, tools_enabled: bool) -> String {
    let model = req
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("stub-model");
    let chunk = |delta: Value, finish: Value| {
        json!({
            "id": "chatcmpl-stub", "object": "chat.completion.chunk", "created": 0, "model": model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]
        })
    };
    let mut events = vec![chunk(json!({"role": "assistant"}), Value::Null)];
    match first_tool(req, tools_enabled) {
        Some(name) => {
            events.push(chunk(
                json!({"tool_calls": [{"index": 0, "id": "call_stub_1", "type": "function",
                    "function": {"name": name, "arguments": "{}"}}]}),
                Value::Null,
            ));
            events.push(chunk(json!({}), json!("tool_calls")));
        }
        None => {
            events.push(chunk(json!({"content": "stub "}), Value::Null));
            events.push(chunk(json!({"content": "reply"}), Value::Null));
            events.push(chunk(json!({}), json!("stop")));
        }
    }
    let mut out = String::new();
    for e in events {
        out.push_str(&format!("data: {e}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

fn main() {
    let port = std::env::var("STUB_PORT").unwrap_or_else(|_| "8000".into());
    let tools_enabled = std::env::var("STUB_TOOLS")
        .map(|v| v != "off")
        .unwrap_or(true);
    let server = tiny_http::Server::http(format!("0.0.0.0:{port}")).expect("bind stub port");
    eprintln!("sesh-stub-llm listening on {port} (tools {tools_enabled})");
    for mut request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("").to_string();
        let method = request.method().as_str().to_string();
        let json_ct = header("Content-Type", "application/json");
        let response = match (method.as_str(), path.as_str()) {
            ("GET", "/health") => tiny_http::Response::from_string("ok"),
            ("GET", "/v1/models") => {
                tiny_http::Response::from_string(models().to_string()).with_header(json_ct)
            }
            ("POST", "/v1/chat/completions") => {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let req: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                if req.get("stream").and_then(Value::as_bool) == Some(true) {
                    tiny_http::Response::from_string(stream(&req, tools_enabled))
                        .with_header(header("Content-Type", "text/event-stream"))
                } else {
                    tiny_http::Response::from_string(completion(&req, tools_enabled).to_string())
                        .with_header(json_ct)
                }
            }
            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };
        let _ = request.respond(response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_two_models() {
        let ids: Vec<_> = models()["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].clone())
            .collect();
        assert_eq!(ids, vec![json!("stub-model"), json!("stub-raw-2")]);
    }

    #[test]
    fn replies_without_tools() {
        let r = completion(&json!({"model": "stub-model", "messages": []}), true);
        assert_eq!(r["choices"][0]["message"]["content"], "stub reply");
        assert_eq!(r["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn calls_the_first_tool() {
        let req =
            json!({"model": "m", "tools": [{"type": "function", "function": {"name": "ping"}}]});
        let r = completion(&req, true);
        assert_eq!(
            r["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "ping"
        );
        assert_eq!(r["choices"][0]["finish_reason"], "tool_calls");
        let off = completion(&req, false);
        assert_eq!(off["choices"][0]["message"]["content"], "stub reply");
    }

    #[test]
    fn streams_and_ends_with_done() {
        let s = stream(&json!({"model": "m", "stream": true}), true);
        assert!(s.contains("\"content\":\"stub \""));
        assert!(s.trim_end().ends_with("data: [DONE]"));
        let t = stream(
            &json!({"model": "m", "tools": [{"function": {"name": "ping"}}]}),
            true,
        );
        assert!(t.contains("\"name\":\"ping\""));
    }
}
