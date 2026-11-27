//! Where the agent-facing listener binds.
//!
//! The gateway is attached to every agent's executor network so executors
//! can reach `:7001`. Binding `:7000` on all interfaces would also put the
//! agent endpoint on those networks, so by default it binds only to
//! loopback (the container healthcheck) and to the address of the
//! interface that holds the default route (the stack's `default` network,
//! where the published port arrives). Executor networks are internal, so
//! they never hold the default route (FR-004, ADR 0004: an executor's
//! single binding to the gateway is the executor port).
//!
//! Since specs/008 the gateway has no `default` network: its only
//! non-internal network is `gateway-edge`, so the default route, and with
//! it this address, is there. Its database, pair, resource and executor
//! networks are all internal, so neither listener ever binds on them
//! (ADR 0011).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

pub const AGENT_PORT: u16 = 7000;

/// The local address the kernel would use to reach the outside, if any.
/// Connecting a UDP socket sends nothing; it only selects a route.
pub fn default_route_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // TEST-NET-1 (RFC 5737): never routed anywhere real.
    s.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// The agent listener's addresses: `SESH_AGENT_LISTEN` (comma separated)
/// when set, else loopback plus the default-route address.
pub fn agent_addrs(
    configured: Option<&str>,
    route_ip: Option<IpAddr>,
) -> Result<Vec<SocketAddr>, String> {
    addrs_on(AGENT_PORT, configured, route_ip)
}

/// The same rule for any port (the approvals listener is `:7002`).
pub fn addrs_on(
    port: u16,
    configured: Option<&str>,
    route_ip: Option<IpAddr>,
) -> Result<Vec<SocketAddr>, String> {
    if let Some(list) = configured {
        return list
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(|a| a.parse().map_err(|_| format!("bad listen address {a}")))
            .collect();
    }
    let mut addrs = vec![SocketAddr::from((Ipv4Addr::LOCALHOST, port))];
    if let Some(ip) = route_ip {
        addrs.push(SocketAddr::new(ip, port));
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_loopback_and_the_default_route_only() {
        let ip: IpAddr = "172.18.0.5".parse().unwrap();
        let addrs = agent_addrs(None, Some(ip)).unwrap();
        assert_eq!(
            addrs,
            vec![
                "127.0.0.1:7000".parse().unwrap(),
                "172.18.0.5:7000".parse().unwrap()
            ]
        );
        assert!(addrs.iter().all(|a| !a.ip().is_unspecified()));
        assert_eq!(
            agent_addrs(None, None).unwrap(),
            vec!["127.0.0.1:7000".parse::<SocketAddr>().unwrap()]
        );
    }

    #[test]
    fn configured_addresses_win() {
        assert_eq!(
            agent_addrs(Some("0.0.0.0:7100, 127.0.0.1:7200"), None).unwrap(),
            vec![
                "0.0.0.0:7100".parse::<SocketAddr>().unwrap(),
                "127.0.0.1:7200".parse().unwrap()
            ]
        );
        assert!(agent_addrs(Some("nope"), None).is_err());
    }

    #[test]
    fn default_route_ip_is_never_a_wildcard_or_loopback() {
        if let Some(ip) = default_route_ip() {
            assert!(!ip.is_unspecified() && !ip.is_loopback(), "{ip}");
        }
    }

    #[test]
    fn with_the_route_on_the_edge_network_both_listeners_bind_loopback_and_it_only() {
        // specs/008 research.md R5: the gateway's edge network holds the
        // default route; nothing else may appear, and never a wildcard.
        let edge: IpAddr = "172.23.0.2".parse().unwrap();
        for port in [AGENT_PORT, 7002] {
            let addrs = addrs_on(port, None, Some(edge)).unwrap();
            assert_eq!(
                addrs,
                vec![
                    SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                    SocketAddr::new(edge, port)
                ]
            );
            assert!(addrs.iter().all(|a| !a.ip().is_unspecified()));
        }
    }
}
