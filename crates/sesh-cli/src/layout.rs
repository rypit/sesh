//! The network layout (specs/008-network-isolation/contracts/networks.md):
//! which networks exist, who is on each, and what each service listens on.
//! It is the one source of truth for the Compose files' unit test, the
//! doctor's `network-layout` check and the `network_stack` reachability
//! test. Executors (`exec-<agent>`) and Kiwix are added per install by
//! the generated overlay and checked by their own tests.

/// A Compose network (data-model.md `Network`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Net {
    pub name: &'static str,
    /// `false` only for edge and egress networks.
    pub internal: bool,
    pub members: &'static [&'static str],
    /// Present only with `compose.stub.yaml` (development and CI).
    pub stub_only: bool,
}

const fn net(name: &'static str, internal: bool, members: &'static [&'static str]) -> Net {
    Net {
        name,
        internal,
        members,
        stub_only: false,
    }
}

/// Every network in the household stack, plus the stub-only `stub`.
pub const NETWORKS: &[Net] = &[
    // A database network per storing service: only it and Postgres.
    net("litellm-db", true, &["postgres", "litellm"]),
    net("openwebui-db", true, &["postgres", "open-webui"]),
    net("gateway-db", true, &["postgres", "gateway"]),
    net("infisical-db", true, &["postgres", "infisical"]),
    net("paperless-db", true, &["postgres", "paperless"]),
    net("feeds-db", true, &["postgres", "miniflux"]),
    // Pair networks: one named link each.
    net("models", true, &["open-webui", "litellm"]),
    net("secrets", true, &["gateway", "infisical"]),
    net("notify", true, &["gateway", "ntfy"]),
    // Edge networks: one member each, for its published port and its
    // outbound access.
    net("webui-edge", false, &["open-webui"]),
    net("litellm-edge", false, &["litellm"]),
    net("gateway-edge", false, &["gateway"]),
    net("infisical-edge", false, &["infisical"]),
    net("ntfy-edge", false, &["ntfy"]),
    // Resource networks (features 002, 005, 006), unchanged.
    net("search", true, &["gateway", "searxng"]),
    net("searxng-egress", false, &["searxng"]),
    net("docs", true, &["gateway", "paperless"]),
    net("feeds", true, &["gateway", "miniflux"]),
    net("feeds-egress", false, &["miniflux"]),
    // Each service's own Redis (feature 007), unchanged.
    net("openwebui-redis", true, &["redis-openwebui", "open-webui"]),
    net("infisical-redis", true, &["redis-infisical", "infisical"]),
    net("paperless-redis", true, &["redis-paperless", "paperless"]),
    // Stub mode: the stub and the services that use its fakes.
    Net {
        name: "stub",
        internal: true,
        members: &["stub-llm", "litellm", "gateway"],
        stub_only: true,
    },
];

/// In stub mode the stub also joins `feeds-egress`, as the fake publisher.
pub const STUB_EXTRA: &[(&str, &str)] = &[("feeds-egress", "stub-llm")];

/// The ports each service listens on, on all of its networks.
pub const LISTENS: &[(&str, u16)] = &[
    ("postgres", 5432),
    ("litellm", 4000),
    ("open-webui", 8080),
    ("gateway", 7000),
    ("gateway", 7001),
    ("gateway", 7002),
    ("infisical", 8080),
    ("ntfy", 80),
    ("paperless", 8000),
    ("miniflux", 8080),
    ("searxng", 8080),
    ("redis-openwebui", 6379),
    ("redis-infisical", 6379),
    ("redis-paperless", 6379),
    ("stub-llm", 8000),
];

/// Listeners bound only to loopback and their service's edge network
/// (the gateway's agent and approvals listeners; research.md R5), so no
/// other container reaches them.
pub const EDGE_ONLY: &[(&str, u16)] = &[("gateway", 7000), ("gateway", 7002)];

/// The paths the stack uses (contracts/networks.md "Allowed paths").
/// Every other reachable pair is a `reverse` path: open because a network
/// carries traffic both ways, but unused.
pub const USED: &[(&str, &str, u16)] = &[
    ("litellm", "postgres", 5432),
    ("open-webui", "postgres", 5432),
    ("open-webui", "litellm", 4000),
    ("open-webui", "redis-openwebui", 6379),
    ("gateway", "postgres", 5432),
    ("gateway", "infisical", 8080),
    ("gateway", "ntfy", 80),
    ("gateway", "searxng", 8080),
    ("gateway", "paperless", 8000),
    ("gateway", "miniflux", 8080),
    ("infisical", "postgres", 5432),
    ("infisical", "redis-infisical", 6379),
    ("paperless", "postgres", 5432),
    ("paperless", "redis-paperless", 6379),
    ("miniflux", "postgres", 5432),
];

/// Used only in stub mode.
pub const USED_STUB: &[(&str, &str, u16)] = &[
    ("litellm", "stub-llm", 8000),
    ("gateway", "stub-llm", 8000),
    ("miniflux", "stub-llm", 8000),
];

/// Every service, in a stable order.
pub fn services(stub: bool) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for n in NETWORKS.iter().filter(|n| stub || !n.stub_only) {
        for m in n.members {
            if !out.contains(m) {
                out.push(m);
            }
        }
    }
    out
}

/// The networks a service is on, in table order.
pub fn networks_of(service: &str, stub: bool) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = NETWORKS
        .iter()
        .filter(|n| (stub || !n.stub_only) && n.members.contains(&service))
        .map(|n| n.name)
        .collect();
    if stub {
        for (n, m) in STUB_EXTRA {
            if *m == service && !out.contains(n) {
                out.push(n);
            }
        }
    }
    out
}

fn members(net: &Net, stub: bool) -> Vec<&'static str> {
    let mut m = net.members.to_vec();
    if stub {
        m.extend(
            STUB_EXTRA
                .iter()
                .filter(|(n, _)| *n == net.name)
                .map(|(_, s)| *s),
        );
    }
    m
}

/// Whether `from` can open a connection to `to:port`: they share a
/// network and `to` listens on that port there.
pub fn reachable(from: &str, to: &str, port: u16, stub: bool) -> bool {
    if from == to || !LISTENS.contains(&(to, port)) || EDGE_ONLY.contains(&(to, port)) {
        return false;
    }
    NETWORKS.iter().filter(|n| stub || !n.stub_only).any(|n| {
        let m = members(n, stub);
        m.contains(&from) && m.contains(&to)
    })
}

/// Every (from, to, port) the test probes, with the expected result.
pub fn matrix(stub: bool) -> Vec<(&'static str, &'static str, u16, bool)> {
    let svcs = services(stub);
    let mut out = vec![];
    for from in &svcs {
        for (to, port) in LISTENS {
            if from != to && svcs.contains(to) {
                out.push((*from, *to, *port, reachable(from, to, *port, stub)));
            }
        }
    }
    out
}

/// The database login rules' inputs (data-model.md `Database login
/// rule`): (role, database, network).
pub const DB_LOGINS: &[(&str, &str, &str)] = &[
    ("litellm", "litellm", "litellm-db"),
    ("openwebui", "openwebui", "openwebui-db"),
    ("sesh_gateway", "gateway", "gateway-db"),
    ("infisical", "infisical", "infisical-db"),
    ("paperless", "paperless", "paperless-db"),
    ("miniflux", "miniflux", "feeds-db"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_no_catch_all_network() {
        assert!(NETWORKS.iter().all(|n| n.name != "default"));
    }

    #[test]
    fn edge_networks_have_one_member_and_no_service_has_two_ways_out() {
        for n in NETWORKS.iter().filter(|n| n.name.ends_with("-edge")) {
            assert!(!n.internal, "{}", n.name);
            assert_eq!(
                n.members.len(),
                1,
                "an edge network has one member: {}",
                n.name
            );
        }
        for stub in [false, true] {
            for s in services(stub) {
                let out: Vec<_> = networks_of(s, stub)
                    .into_iter()
                    .filter(|n| NETWORKS.iter().any(|x| x.name == *n && !x.internal))
                    .collect();
                assert!(
                    out.len() <= 1,
                    "{s} has more than one non-internal network: {out:?}"
                );
            }
        }
    }

    #[test]
    fn postgres_is_only_on_database_networks() {
        for n in networks_of("postgres", true) {
            assert!(n.ends_with("-db"), "postgres is on {n}");
        }
        let db: Vec<_> = DB_LOGINS.iter().map(|(_, _, n)| *n).collect();
        let mut on: Vec<_> = networks_of("postgres", false);
        on.sort();
        let mut want = db.clone();
        want.sort();
        assert_eq!(on, want, "one database network per login");
    }

    #[test]
    fn every_used_path_is_reachable() {
        for (f, t, p) in USED {
            assert!(reachable(f, t, *p, false), "{f} -> {t}:{p}");
        }
        for (f, t, p) in USED_STUB {
            assert!(reachable(f, t, *p, true), "stub: {f} -> {t}:{p}");
            assert!(
                !reachable(f, t, *p, false),
                "{f} -> {t}:{p} only in stub mode"
            );
        }
    }

    #[test]
    fn the_lateral_paths_that_motivated_this_are_closed() {
        for (f, t, p) in [
            ("open-webui", "infisical", 8080),
            ("open-webui", "gateway", 7002),
            ("open-webui", "ntfy", 80),
            ("litellm", "gateway", 7000),
            ("litellm", "gateway", 7001),
            ("ntfy", "postgres", 5432),
            ("ntfy", "infisical", 8080),
            ("paperless", "infisical", 8080),
            ("miniflux", "gateway", 7002),
            ("open-webui", "redis-infisical", 6379),
        ] {
            assert!(!reachable(f, t, p, false), "{f} -> {t}:{p} must be closed");
        }
    }

    #[test]
    fn the_gateways_agent_and_approval_ports_reach_no_container() {
        for s in services(true) {
            for p in [7000, 7002] {
                assert!(!reachable(s, "gateway", p, true), "{s} -> gateway:{p}");
            }
        }
    }

    #[test]
    fn reverse_paths_are_the_shared_network_ones() {
        // A two-way network makes these reachable though unused; listing
        // them keeps a change visible.
        assert!(reachable("litellm", "open-webui", 8080, false));
        assert!(reachable("infisical", "gateway", 7001, false));
        assert!(!USED.contains(&("litellm", "open-webui", 8080)));
    }

    #[test]
    fn db_logins_name_real_networks_shared_with_postgres() {
        for (role, db, net) in DB_LOGINS {
            let n = NETWORKS
                .iter()
                .find(|n| n.name == *net)
                .unwrap_or_else(|| panic!("{net}"));
            assert!(
                n.internal && n.members.contains(&"postgres") && n.members.len() == 2,
                "{role}/{db}"
            );
        }
    }
}
