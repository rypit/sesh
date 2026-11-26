//! Seam: the Tailscale CLI.

use anyhow::Result;
use serde_json::Value;
use std::process::Command;

pub trait Tailscale {
    /// `None` when Tailscale isn't installed or isn't running; otherwise
    /// whether `tailscale serve` proxies to `127.0.0.1:<port>`.
    fn serves_port(&self, port: u16) -> Result<Option<bool>>;
}

pub struct TailscaleCli;

impl Tailscale for TailscaleCli {
    fn serves_port(&self, port: u16) -> Result<Option<bool>> {
        let Ok(out) = Command::new("tailscale")
            .args(["serve", "status", "--json"])
            .output()
        else {
            return Ok(None);
        };
        if !out.status.success() {
            return Ok(None);
        }
        let status: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        Ok(Some(serves(&status, port)))
    }
}

/// True if any proxy target in `tailscale serve status --json` points at
/// the given local port.
pub fn serves(status: &Value, port: u16) -> bool {
    let needles = [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ];
    let mut found = false;
    walk(status, &mut |s| {
        if needles.iter().any(|n| s.contains(n.as_str())) {
            found = true;
        }
    });
    found
}

fn walk(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
        Value::Object(o) => o.values().for_each(|x| walk(x, f)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_proxy_target() {
        let s: Value = serde_json::from_str(
            r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"host.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:3000"}}}}}"#,
        )
        .unwrap();
        assert!(serves(&s, 3000));
        assert!(!serves(&s, 4000));
        assert!(!serves(&Value::Null, 3000));
    }
}
