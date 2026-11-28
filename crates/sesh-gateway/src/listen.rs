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

//!
//! Since specs/009 the agent listener also binds on each agent runtime's
//! own network (`agent-<name>`, ADR 0012). `gateway.toml` names the
//! gateway's alias there (`agent_listen`); Docker's DNS answers it with
//! the gateway's address on that network only. An alias that doesn't
//! resolve to a usable address stops the gateway: it never widens the
//! binding to make up for it.

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

/// Adds the agent networks' addresses (each alias with what it resolved
/// to) to the agent listener's addresses.
pub fn with_agent_networks(
    mut addrs: Vec<SocketAddr>,
    resolved: &[(String, Vec<IpAddr>)],
) -> Result<Vec<SocketAddr>, String> {
    for (alias, ips) in resolved {
        let usable: Vec<&IpAddr> = ips
            .iter()
            .filter(|ip| ip.is_ipv4() && !ip.is_unspecified() && !ip.is_loopback())
            .collect();
        if usable.is_empty() {
            return Err(format!(
                "the agent network alias {alias} has no address on this container; not starting"
            ));
        }
        for ip in usable {
            let a = SocketAddr::new(*ip, AGENT_PORT);
            if !addrs.contains(&a) {
                addrs.push(a);
            }
        }
    }
    Ok(addrs)
}

/// Resolves each alias, retrying while Docker attaches the networks.
pub async fn resolve_aliases(aliases: &[String]) -> Result<Vec<(String, Vec<IpAddr>)>, String> {
    let mut out = vec![];
    for alias in aliases {
        let mut ips = vec![];
        for _ in 0..30 {
            if let Ok(found) = tokio::net::lookup_host((alias.as_str(), AGENT_PORT)).await {
                ips = found.map(|a| a.ip()).collect();
                if !ips.is_empty() {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        out.push((alias.clone(), ips));
    }
    Ok(out)
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

    #[test]
    fn agent_networks_add_exact_addresses_never_a_wildcard() {
        let base = addrs_on(AGENT_PORT, None, Some("172.23.0.2".parse().unwrap())).unwrap();
        let resolved = vec![
            (
                "gateway-agent-a".to_string(),
                vec!["172.40.0.3".parse().unwrap()],
            ),
            (
                "gateway-agent-b".to_string(),
                vec!["172.41.0.2".parse().unwrap(), "::1".parse().unwrap()],
            ),
        ];
        let addrs = with_agent_networks(base.clone(), &resolved).unwrap();
        assert_eq!(addrs.len(), base.len() + 2);
        assert!(addrs.contains(&"172.40.0.3:7000".parse().unwrap()));
        assert!(addrs.contains(&"172.41.0.2:7000".parse().unwrap()));
        assert!(addrs.iter().all(|a| !a.ip().is_unspecified()));
        assert_eq!(with_agent_networks(base.clone(), &[]).unwrap(), base);
    }

    #[test]
    fn an_alias_without_an_address_stops_the_gateway() {
        for bad in [
            vec![],
            vec!["0.0.0.0".parse().unwrap()],
            vec!["127.0.0.1".parse().unwrap()],
        ] {
            let e = with_agent_networks(vec![], &[("gateway-agent-a".into(), bad)]).unwrap_err();
            assert!(
                e.contains("gateway-agent-a") && e.contains("not starting"),
                "{e}"
            );
        }
    }
}
