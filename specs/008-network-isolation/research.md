# Research: Network isolation

## R1. Published ports need a non-internal network

- **Decision:** every service that publishes a loopback port (Open
  WebUI, LiteLLM, the gateway, Infisical, ntfy) gets its own
  non-internal `*-edge` network, with no other member.
- **Rationale:** Docker publishes a container's ports only through a
  non-internal network. On internal networks alone, a published port
  silently doesn't answer. One member per edge network means it is
  never a path between services.
- **Alternatives:**
  - **One shared "edge" network:** that rebuilds `default`. Rejected.
  - **A host-network sidecar or proxy** for the ports: a new component
    to adopt and secure. Rejected.

## R2. One non-internal network per service

- **Decision:** no service joins more than one non-internal network.
  The layout table enforces this, and so do a unit test and the doctor.
- **Rationale:** internal networks have no gateway, so a container's
  default route goes through its only non-internal network. That makes
  its outbound path unambiguous, and so is the address the gateway's
  listeners bind (R5).
- **Existing exceptions:**
  - Miniflux is on `feeds-egress` (non-internal) alone, which is fine.
  - In stub mode, `stub-llm` is on `feeds-egress` (non-internal) and
    `stub` (internal), which is fine.

## R3. Who needs outbound access

| Service | Outbound need | Today |
|---|---|---|
| LiteLLM | the inference server: LAN, the host via `host.docker.internal`, or a hosted API | via `default` |
| gateway | Google APIs, Open-Meteo, Nominatim | via `default` |
| ntfy | upstream push relays (optional) | via `default` |
| Open WebUI | none (`OFFLINE_MODE`) | via `default`; still possible through `webui-edge` |
| Infisical | none known | via `default`; still possible through `infisical-edge` |

- **Decision:** an edge network gives general outbound access, and we
  accept that for Open WebUI and Infisical. Compose can't express
  egress filtering; host firewall rules are a recorded follow-up.
- **`host.docker.internal`:** `host-gateway` resolves to the host's
  bridge address, which is reachable from any user-defined bridge, so
  LiteLLM's `extra_hosts` keeps working on `litellm-edge`.

## R4. The upgrade: recreate containers, then remove `sesh_default`

- **Decision:** `sesh up` runs `docker compose up -d --wait
  --remove-orphans` as today. Compose recreates any container whose
  networks changed. Then `sesh up` removes the network `sesh_default`
  if it exists and no container is attached; otherwise it reports which
  containers still use it.
- **Rationale:**
  - Compose doesn't remove networks it no longer declares; only `down`
    does, and `down` would stop the stack.
  - Volumes are untouched, so no data changes.
- **Alternative:** `sesh down` then `sesh up`. More downtime for the
  same result. Rejected.

## R5. The gateway's listener rule

- **Facts:**
  - `listen.rs` binds the agent listener (`:7000`) and the approvals
    listener (`:7002`) on loopback plus the address of the interface
    that holds the default route. The executor listener (`:7001`)
    binds all addresses.
  - Today the default route is `default`.
  - After this feature, the gateway's only non-internal network is
    `gateway-edge` (R2), so its default route is there.
- **Decision:**
  - Keep the code.
  - Add a unit test for the rule that matters: with the route address
    on an edge network, the agent and approvals addresses are loopback
    plus that address and nothing else.
  - Have the stack test check, from Open WebUI, LiteLLM, Paperless,
    Miniflux and an executor, that `:7000` and `:7002` aren't
    reachable.
- **The executor port** is reachable from the gateway's other internal
  networks too, such as `gateway-db`. Feature 006's check rejects a
  call that doesn't come from the calling agent's own executor (ADR
  0009, decision 5). The reachability test includes `:7001` from
  non-executors as "connects but every call is refused", which is
  already covered by `feeds_stack` and the unit tests.

## R6. The stub in stub mode

- **Who uses it:**
  - LiteLLM uses it as the model;
  - the gateway uses its fake Google, search, weather, maps and test
    upstreams;
  - Miniflux uses it as the fake feed publisher.
- **Decision:** `compose.stub.yaml` defines an internal `stub` network
  joining `stub-llm`, `litellm` and `gateway`. `stub-llm` also stays on
  `feeds-egress` for Miniflux. The household `compose.yaml` has no
  `stub`.
- **Rationale:** it adds no path between household services: `stub`
  holds only the stub and its clients, and only in development and CI.

## R7. One source of truth for the layout

- **Decision:** `crates/sesh-cli/src/layout.rs` holds:
  - `NETWORKS`: each network, whether it's internal, and its members,
    with stub-only networks marked;
  - `REACH`: the allowed (from, to, port) triples, which follow from
    the table.

  The checks built on it:
  - a unit test that parses `compose.yaml` and `compose.stub.yaml` and
    asserts that every service's networks and every network's
    `internal` flag match;
  - the doctor's `network-layout` check, which compares the running
    containers' networks (from `docker inspect`) with the table;
  - `network_stack`, which probes every pair.
- **Rationale:** one table means a new service can't be added to
  Compose without the test, the doctor and the reachability check
  knowing about it.

## R8. How reachability is probed

- **Method:**
  - Each probe runs in the source container's network namespace, from
    the gateway image: `docker run --network container:<id>` with bash
    and `/dev/tcp`. This is feature 006's `probe_from`, and it doesn't
    depend on tools inside each image.
  - Disallowed destinations are probed by **every address** they have
    on any network, not just by name. Otherwise a name that doesn't
    resolve would make the test pass trivially.
- **Ports per destination:**

  | Destination | Ports |
  |---|---|
  | postgres | 5432 |
  | litellm | 4000 |
  | open-webui | 8080 |
  | gateway | 7000, 7001, 7002 |
  | infisical | 8080 |
  | ntfy | 80 |
  | paperless | 8000 |
  | miniflux | 8080 |
  | searxng | 8080 |
  | each redis | 6379 |

- **Expected result:** computed from the table. A pair connects if and
  only if the two containers share a network and the destination
  listens there; the only exception is the gateway's loopback-and-edge
  listeners. Networks are two-way, so the table also lists the unused
  reverse paths (contracts/networks.md), and the test fails if any
  path appears or disappears.
- **Positive controls:** every allowed pair must connect.
  - In stub mode, LiteLLM → stub-llm:8000 and gateway → stub-llm:8000
    are allowed.
  - Executors keep feature 002's checks: `exec-<agent>` reaches only
    `gateway:7001`.

## R9. Roadmap

008 network isolation, 009 read-only Google Docs, 010 memory, 011 one
agent, 012 browser, 013 household. Updated in the README, AGENTS.md and
the architecture doc's phase list.

## Follow-ups (recorded, not built)

- **Postgres `pg_hba`:** bind each login to its database network's
  subnet. That needs fixed subnets, at the risk of clashing with the
  household's LAN or VPN.
- **Egress filtering** for Open WebUI and Infisical, with host firewall
  rules (`DOCKER-USER`).
