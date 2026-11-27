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

## R10. Database logins bound to their networks

- **Decision:**
  - Postgres starts with `-c hba_file=/etc/sesh-pg/pg_hba.conf`. That
    file sits in a directory sesh owns: `${SESH_DATA_DIR}/postgres-conf`,
    mounted read-only. It is a directory mount, because a single-file
    bind mount keeps the old file when the host replaces it.
  - The file is generated: it never takes the image's default
    `host all all all scram-sha-256`.
  - On every `sesh up`, after `up postgres`, sesh:
    1. reads each `*-db` network's subnets with `docker network inspect`
       (IPv4, and IPv6 if enabled);
    2. writes one line per login: `host <db> <role> <subnet> scram-sha-256`;
    3. adds `local all all trust` for administration through `docker
       compose exec`, as the image does today;
    4. adds a final `host all all all reject`;
    5. writes the file atomically within the directory and runs
       `SELECT pg_reload_conf()` through the local socket.
- **Before the first `sesh up` writes rules** (a fresh volume, or a
  missing file), the file holds only the local line plus the reject.
  Init scripts run over the local socket, so first boot works, and no
  service can log in until the rules exist.
- **Fails closed:** if a range can't be read, sesh writes no line for
  that login and `sesh up` fails, naming the network. Nothing falls back
  to "allow everywhere".
- **Why the ranges are stable enough:** Docker keeps a network's subnet
  for the network's whole life. `sesh up` regenerates the rules every
  run, so a recreated network (after `sesh down`, say) is picked up.
- **Alternatives:**
  - **Fixed subnets in Compose IPAM:** these can clash with the
    household's LAN, VPN or other Docker networks, and Compose then
    refuses to start. Rejected.
  - **Hostnames in `pg_hba`,** using reverse DNS from Docker's resolver:
    container names and aliases vary, and a DNS failure would decide
    access. Rejected.
- **The gateway's login** (`sesh_gateway`) is bound to `gateway-db`, and
  `miniflux` to `feeds-db`, like the rest.

## Follow-ups (recorded, not built)

- **Egress filtering** for Open WebUI and Infisical, with host firewall
  rules (`DOCKER-USER`).

## CI results (T022)

- **PR #14, pull_request run 46057894533** (head `27407f2`): every job
  that ran passed.
  - `changes` and `images` passed.
  - The three stack jobs passed: `eval_stack gateway_stack network_stack`,
    `isolation_stack secrets_stack`, and
    `foundation_stack google_docs_stack feeds_stack`.
  - `check` was skipped, by design, on pull_request runs; the branch's
    push run covers it.
- **main after the merge of #14, push run 46060058391** (`542bf2b`):
  every job passed (`check`, `changes`, `images` and all three stack
  jobs). Nothing on main needed fixing.
- **The follow-up PR #15** (T015, T017, T019, T020):
  - **Run 46061755259 failed.** `network_stack` failed in the new
    pre-008 rewind. The fresh-install checks and the doctor drift step
    (T019) had passed.
    - The cause was in the test, not in `sesh`. Once the services were
      back on `default`, they reached Postgres from their `default`
      addresses, which the current login rules refuse, so Infisical and
      LiteLLM never became healthy.
    - The fix: the test now writes the permissive pre-008 rules before
      the rewind, which is how a pre-008 install was.
  - **Pull_request run 46065044778** (head `30ff2f4`): every job passed,
    including `network_stack` with the upgrade from pre-008 and the
    second `sesh up`. `check` passed in push run 46065039474.
