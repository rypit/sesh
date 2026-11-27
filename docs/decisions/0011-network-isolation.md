# 0011. No shared network; each database login bound to its own network

- Status: proposed
- Date: 2026-11-27
- Feature: specs/008-network-isolation

## Context

Feature 007 gave each adopted service its own database login and its
own Redis, so a compromised service can't read another's data. But
seven services (Postgres, LiteLLM, Open WebUI, the gateway, Infisical,
ntfy and Paperless) still shared Compose's `default` network, so each
could open a connection to every other's ports:
- Open WebUI could reach Infisical's API and the gateway's approval
  pages;
- LiteLLM could reach the gateway;
- ntfy could reach Postgres.

Any container that reached Postgres could also try any login's
password.

## Decision

1. **No `default` network.** Every service names its networks, as
   listed in `crates/sesh-cli/src/layout.rs`, the single source of truth
   (specs/008 contracts/networks.md). A unit test checks it against the
   Compose files, and `sesh doctor` (`network-layout`) and the
   `network_stack` stack test check it against the running stack.
2. **Three kinds of network:**
   - **A database network** per storing service, shared only by that
     service and Postgres. Postgres is on nothing else.
   - **A pair network** per remaining link: `models` (Open WebUI and
     LiteLLM), `secrets` (the gateway and Infisical) and `notify` (the
     gateway and ntfy).
   - **An edge network** per service with a published loopback port or
     an outbound need, with that service as its only member.

   The resource, cache and executor networks are unchanged.
3. **At most one non-internal network per service.** Docker publishes
   ports only through a non-internal network, and internal networks
   have no route out. So each service's default route, and its only way
   out, is its edge network.
4. **The gateway's listener rule is unchanged.** Its agent (`:7000`) and
   approvals (`:7002`) listeners bind loopback plus the default-route
   address, which is now `gateway-edge`'s, where no other container is.
   A unit test pins the rule. The executor port (`:7001`) still accepts
   connections on all the gateway's networks and refuses every call not
   from the calling agent's own executor (ADR 0009, decision 5).
5. **Each database login is accepted only from its own network.**
   - Postgres reads its login rules (`hba_file`) from a sesh-owned
     directory, mounted read-only.
   - On every `sesh up`, sesh reads each database network's subnets from
     Docker. It allows each login only from those ranges, the superuser
     only over the container's local socket, and rejects everything
     else, then reloads Postgres.
   - No ranges are fixed in configuration, so nothing can clash with
     the household's LAN or VPN.
   - If a range can't be read, `sesh up` fails and that login stays
     refused.
6. **Stub mode** adds an internal `stub` network for the stub and its
   clients (LiteLLM and the gateway). The household configuration
   doesn't contain it.
7. **The upgrade is `sesh up`.** Compose recreates containers on their
   new networks, and `sesh up` removes `sesh_default` once nothing uses
   it. Volumes are untouched.

## Alternatives

- **Fixed subnets** in Compose for the database networks, so the login
  rules could be static. They can clash with the household's networks,
  and Compose then refuses to start. Rejected in favour of reading the
  assigned ranges.
- **Hostnames in the login rules**, through Docker's reverse DNS.
  Container names vary, and a DNS failure would decide access. Rejected.
- **One shared edge network** for every published port: that rebuilds
  `default`. Rejected.
- **A reverse proxy** in front of the published ports: a new component,
  with no gain over single-member edge networks. Rejected.

## Consequences

- **Threat model:** a compromised service reaches only its row of the
  layout table. Its edge network gives it outbound access; that's
  needed for LiteLLM, the gateway and ntfy, and merely possible for
  Open WebUI (in offline mode) and Infisical. Narrowing it needs host
  firewall rules, which is a recorded follow-up.
- **Shared networks carry traffic both ways.** So a few unused reverse
  paths stay open (for example LiteLLM to Open WebUI on `models`). The
  layout table lists them, and the stack test expects exactly them.
- **A network recreated by hand** can get a new range; its login is
  refused until the next `sesh up`. That fails closed.
- **Adding a service** means adding it to `layout.rs` first; the unit
  test fails until Compose matches.
- About 14 networks instead of 10, with negligible overhead.
