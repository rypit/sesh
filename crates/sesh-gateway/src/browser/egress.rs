//! The browser's only way online (specs/010 research.md R4, ADR 0013):
//! an HTTP proxy that resolves each host itself, refuses home-network
//! addresses, and connects to the address it checked (so DNS rebinding
//! can't switch it afterwards).
//!
//! Chromium uses it for everything: navigation, redirects, subresources
//! and WebSockets. Plain HTTP requests are forwarded one per connection
//! (`Connection: close`), so a kept-alive connection can't be reused for
//! another host; HTTPS and WebSockets use `CONNECT`.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Whether the browser may connect to `ip`: never a private, shared
/// (CGNAT, which includes Tailscale), loopback, link-local, unique-local,
/// multicast, unspecified, documentation or metadata address.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_v4(v4);
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (s[0] & 0xffc0) == 0xfec0 // site-local (deprecated)
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x0064 && s[1] == 0xff9b) // NAT64 (could map to private v4)
                || s[0] == 0x2002) // 6to4 (embeds a v4 address)
        }
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local() // 169.254/16, including cloud metadata
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0 // 0.0.0.0/8
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10: CGNAT, Tailscale
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 benchmarking
        || o[0] >= 240) // reserved
}

/// The refusal text an agent sees (contracts/browser-connector.md).
pub const REFUSED: &str = "that address is on the home network; the browser can't open it";

/// Where a host may connect: its resolved addresses that are public, or
/// in `allow` (stub mode only: the fake sites' own addresses), in order.
/// Refused addresses are dropped, never tried.
pub fn choose_all(resolved: &[IpAddr], allow: &[IpAddr]) -> Vec<IpAddr> {
    resolved
        .iter()
        .copied()
        .filter(|ip| public(*ip) || allow.contains(ip))
        .collect()
}

/// The first address [`choose_all`] allows.
pub fn choose(resolved: &[IpAddr], allow: &[IpAddr]) -> Option<IpAddr> {
    choose_all(resolved, allow).into_iter().next()
}

/// Refusals the proxy made, per agent, for the connector to report.
#[derive(Default, Clone)]
pub struct Refusals(Arc<Mutex<HashMap<String, VecDeque<String>>>>);

impl Refusals {
    pub fn note(&self, agent: &str, host: &str) {
        let mut m = self.0.lock().expect("lock");
        let q = m.entry(agent.to_string()).or_default();
        q.push_back(host.to_string());
        while q.len() > 50 {
            q.pop_front();
        }
    }

    /// Takes the hosts refused for `agent` since the last call.
    pub fn take(&self, agent: &str) -> Vec<String> {
        self.0
            .lock()
            .expect("lock")
            .remove(agent)
            .map(Vec::from)
            .unwrap_or_default()
    }
}

/// A request head as the proxy needs it.
#[derive(Debug, PartialEq)]
pub enum Head {
    /// `CONNECT host:port`
    Connect { host: String, port: u16 },
    /// An absolute-form request: forwarded with its request line in origin
    /// form and `Connection: close`.
    Forward {
        host: String,
        port: u16,
        head: Vec<u8>,
    },
}

/// Parses the proxy request head (everything up to the blank line).
pub fn parse_head(raw: &[u8]) -> Result<Head, &'static str> {
    let text = std::str::from_utf8(raw).map_err(|_| "bad request")?;
    let mut lines = text.split("\r\n");
    let first = lines.next().ok_or("bad request")?;
    let mut parts = first.split(' ');
    let (method, target, version) = (
        parts.next().ok_or("bad request")?,
        parts.next().ok_or("bad request")?,
        parts.next().ok_or("bad request")?,
    );
    if !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err("bad request");
    }
    if method == "CONNECT" {
        let (host, port) = split_host_port(target, 443)?;
        return Ok(Head::Connect { host, port });
    }
    let url = url::Url::parse(target).map_err(|_| "only absolute http addresses")?;
    if url.scheme() != "http" {
        return Err("only http and https addresses can be opened");
    }
    let host = url
        .host_str()
        .ok_or("no host")?
        .trim_matches(['[', ']'])
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let mut path = url.path().to_string();
    if let Some(q) = url.query() {
        path.push('?');
        path.push_str(q);
    }
    let mut out = format!("{method} {path} {version}\r\n");
    for l in lines.filter(|l| !l.is_empty()) {
        let name = l
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if name == "connection"
            || name == "proxy-connection"
            || name == "keep-alive"
            || name.starts_with("proxy-")
        {
            continue;
        }
        out.push_str(l);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    Ok(Head::Forward {
        host,
        port,
        head: out.into_bytes(),
    })
}

fn split_host_port(target: &str, default: u16) -> Result<(String, u16), &'static str> {
    if let Some(rest) = target.strip_prefix('[') {
        let (h, p) = rest.split_once(']').ok_or("bad host")?;
        let port = match p.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| "bad port")?,
            None => default,
        };
        return Ok((h.to_string(), port));
    }
    match target.rsplit_once(':') {
        Some((h, p)) => Ok((h.to_string(), p.parse().map_err(|_| "bad port")?)),
        None => Ok((target.to_string(), default)),
    }
}

/// Resolves `host` to its allowed addresses (see [`choose_all`]); a host
/// with none is refused.
pub async fn resolve(host: &str, port: u16, allow: &[IpAddr]) -> Result<Vec<SocketAddr>, String> {
    let resolved: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| format!("can't find {host}"))?
            .map(|a| a.ip())
            .collect(),
    };
    let ok: Vec<SocketAddr> = choose_all(&resolved, allow)
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect();
    if ok.is_empty() {
        return Err(REFUSED.to_string());
    }
    Ok(ok)
}

/// Connects to the first of `addrs` that answers (a dual-stack host may
/// listen on only one family).
async fn connect_any(addrs: &[SocketAddr]) -> Option<TcpStream> {
    for a in addrs {
        if let Ok(Ok(s)) =
            tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(a)).await
        {
            return Some(s);
        }
    }
    None
}

/// What the proxy listener needs.
#[derive(Clone)]
pub struct Proxy {
    pub agent: String,
    /// Hostnames whose addresses are allowed though private (stub mode).
    pub allow_hosts: Vec<String>,
    pub refusals: Refusals,
}

impl Proxy {
    async fn allowed_ips(&self) -> Vec<IpAddr> {
        let mut out = vec![];
        for h in &self.allow_hosts {
            if let Ok(a) = tokio::net::lookup_host((h.as_str(), 80)).await {
                out.extend(a.map(|a| a.ip()));
            }
        }
        out
    }

    /// Serves one agent's browser on `listener` until the task is dropped.
    pub async fn serve(self, listener: TcpListener) {
        loop {
            let Ok((client, _)) = listener.accept().await else {
                continue;
            };
            let p = self.clone();
            tokio::spawn(async move {
                let _ = p.handle(client).await;
            });
        }
    }

    async fn handle(&self, mut client: TcpStream) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            let n = tokio::time::timeout(Duration::from_secs(30), client.read(&mut chunk))
                .await
                .map_err(|_| std::io::ErrorKind::TimedOut)??;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            if buf.len() > 64 * 1024 {
                return reply(&mut client, 431, "request head too large").await;
            }
        };
        let head = match parse_head(&buf[..head_end]) {
            Ok(h) => h,
            Err(why) => return reply(&mut client, 400, why).await,
        };
        let (host, port) = match &head {
            Head::Connect { host, port } | Head::Forward { host, port, .. } => {
                (host.clone(), *port)
            }
        };
        let allow = self.allowed_ips().await;
        let addrs = match resolve(&host, port, &allow).await {
            Ok(a) => a,
            Err(why) => {
                if why == REFUSED {
                    self.refusals.note(&self.agent, &host);
                    tracing::info!(agent = %self.agent, host = %host, "browser egress refused: home network");
                }
                return reply(&mut client, 403, &why).await;
            }
        };
        let Some(mut upstream) = connect_any(&addrs).await else {
            return reply(&mut client, 502, "can't connect").await;
        };
        match head {
            Head::Connect { .. } => {
                client
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await?;
                if buf.len() > head_end {
                    upstream.write_all(&buf[head_end..]).await?;
                }
            }
            Head::Forward { head, .. } => {
                upstream.write_all(&head).await?;
                if buf.len() > head_end {
                    upstream.write_all(&buf[head_end..]).await?;
                }
            }
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
        Ok(())
    }
}

async fn reply(client: &mut TcpStream, status: u16, why: &str) -> std::io::Result<()> {
    let reason = match status {
        400 => "Bad Request",
        403 => "Forbidden",
        431 => "Request Header Fields Too Large",
        _ => "Bad Gateway",
    };
    let body = format!("{why}\n");
    client
        .write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
}

/// Is `ip` an address of the stub's fake sites? (Used only through
/// `allow_hosts`; here for completeness of the rules' tests.)
#[cfg(test)]
fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_network_addresses_are_refused() {
        for ip in [
            v4(10, 0, 0, 1),
            v4(172, 16, 5, 4),
            v4(192, 168, 1, 1),
            v4(127, 0, 0, 1),
            v4(169, 254, 169, 254),
            v4(100, 100, 100, 100), // Tailscale
            v4(100, 64, 0, 1),
            v4(0, 0, 0, 0),
            v4(0, 1, 2, 3),
            v4(255, 255, 255, 255),
            v4(224, 0, 0, 1),
            v4(192, 0, 2, 1),
            v4(198, 18, 0, 1),
            v4(240, 0, 0, 1),
        ] {
            assert!(!public(ip), "{ip} must be refused");
        }
        for s in [
            "::1",
            "::",
            "fc00::1",
            "fd7a:115c:a1e0::1", // Tailscale's IPv6
            "fe80::1",
            "::ffff:192.168.1.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "64:ff9b::a00:1",
            "2002:a00:1::1",
            "ff02::1",
        ] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(!public(ip), "{s} must be refused");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for s in [
            "93.184.216.34",
            "1.1.1.1",
            "100.128.0.1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(public(s.parse().unwrap()), "{s}");
        }
    }

    #[test]
    fn a_host_is_used_only_through_an_allowed_address() {
        let private = v4(10, 0, 0, 1);
        let pubv = v4(93, 184, 216, 34);
        assert_eq!(choose(&[private, pubv], &[]), Some(pubv));
        assert_eq!(choose(&[private], &[]), None);
        assert_eq!(
            choose(&[private], &[private]),
            Some(private),
            "stub mode only"
        );
        assert_eq!(choose(&[v4(10, 0, 0, 2)], &[private]), None);
        assert_eq!(
            choose_all(&[private, pubv, v4(1, 1, 1, 1)], &[]),
            vec![pubv, v4(1, 1, 1, 1)],
            "every allowed address, in order; refused ones never tried"
        );
    }

    #[test]
    fn heads_are_parsed_and_rewritten_for_one_request_per_connection() {
        assert_eq!(
            parse_head(b"CONNECT example.org:443 HTTP/1.1\r\nHost: example.org:443\r\n\r\n")
                .unwrap(),
            Head::Connect {
                host: "example.org".into(),
                port: 443
            }
        );
        assert_eq!(
            parse_head(b"CONNECT [2606:4700::1111]:443 HTTP/1.1\r\n\r\n").unwrap(),
            Head::Connect {
                host: "2606:4700::1111".into(),
                port: 443
            }
        );
        let Head::Forward { host, port, head } = parse_head(
            b"GET http://fake.test:8000/a?b=1 HTTP/1.1\r\nHost: fake.test:8000\r\nProxy-Connection: keep-alive\r\nConnection: keep-alive\r\nAccept: */*\r\n\r\n",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!((host.as_str(), port), ("fake.test", 8000));
        let h = String::from_utf8(head).unwrap();
        assert!(h.starts_with("GET /a?b=1 HTTP/1.1\r\n"), "{h}");
        assert!(h.contains("Host: fake.test:8000\r\n") && h.contains("Accept: */*\r\n"));
        assert!(!h.to_lowercase().contains("keep-alive"));
        assert!(h.ends_with("Connection: close\r\n\r\n"));
        assert!(parse_head(b"GET ftp://x/ HTTP/1.1\r\n\r\n").is_err());
        assert!(parse_head(b"GET /relative HTTP/1.1\r\n\r\n").is_err());
        assert!(parse_head(b"nonsense\r\n\r\n").is_err());
    }

    #[tokio::test]
    async fn the_proxy_refuses_home_addresses_and_records_them() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let refusals = Refusals::default();
        let p = Proxy {
            agent: "a".into(),
            allow_hosts: vec![],
            refusals: refusals.clone(),
        };
        tokio::spawn(p.serve(l));
        for req in [
            "CONNECT 10.0.0.1:443 HTTP/1.1\r\n\r\n",
            "GET http://127.0.0.1:9/ HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            "CONNECT localhost:443 HTTP/1.1\r\n\r\n",
        ] {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            assert!(out.starts_with("HTTP/1.1 403"), "{req}: {out}");
            assert!(out.contains("home network"));
        }
        let r = refusals.take("a");
        assert_eq!(r, vec!["10.0.0.1", "127.0.0.1", "localhost"]);
        assert!(refusals.take("a").is_empty());
    }

    #[tokio::test]
    async fn the_proxy_forwards_to_an_allowed_address() {
        // A local "site", allowed the way stub mode allows the fakes.
        let site = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site_port = site.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = site.accept().await.unwrap();
            let mut buf = vec![0u8; 2048];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body =
                if req.starts_with("GET /hello HTTP/1.1") && req.contains("Connection: close") {
                    "hi"
                } else {
                    "bad"
                };
            s.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        });
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(
            Proxy {
                agent: "a".into(),
                allow_hosts: vec!["127.0.0.1".into()],
                refusals: Refusals::default(),
            }
            .serve(l),
        );
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(
            format!("GET http://127.0.0.1:{site_port}/hello HTTP/1.1\r\nHost: 127.0.0.1\r\nProxy-Connection: keep-alive\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert!(out.ends_with("hi"), "{out}");
    }
}
