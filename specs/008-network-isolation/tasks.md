---
description: "Task list for feature 008: network isolation"
---

# Tasks: Network isolation

**Input**: `specs/008-network-isolation/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; spec FR-013–FR-015). Each unit test is written failing before its code; `network_stack` is written before the Compose change it checks.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [x] T001 [P] ADR `docs/decisions/0011-network-isolation.md`:
  - the layout (contracts/networks.md);
  - why each service has one non-internal network (research.md R1, R2);
  - the gateway listener rule and why it holds unchanged (R5);
  - the stub network (R6);
  - database logins bound to their networks' actual subnets, regenerated on every `sesh up` (R10);
  - follow-up: egress filtering.

  Status proposed.
- [x] T002 [P] Roadmap renumbering (008 network isolation, 009 read-only Google Docs, 010 memory, 011 one agent, 012 browser, 013 household) in `README.md`, `AGENTS.md` and the phase list in `docs/architecture/solutions-architecture.md`

---

## Phase 2: Foundational

- [x] T003 Unit tests first, then `crates/sesh-cli/src/layout.rs`: the layout table per data-model.md.
  - **Fields:** `Network.name`, `Network.internal`, `Network.members`, `Network.stub_only`; `Allowed path.from`, `Allowed path.to`, `Allowed path.port`, `Allowed path.stub_only`, `Allowed path.kind`.
  - **Values:** the networks, services and paths in contracts/networks.md.
  - **Tests check the rules:**
    - no `default` network;
    - every edge network has one member (`Network.members`);
    - no service is on more than one non-internal network (`Network.internal`);
    - Postgres is only on `*-db` networks;
    - every `used` path is between services that share a network;
    - every pair sharing a network has a `used` or `reverse` path for each port the destination listens on, except the gateway's `:7000` and `:7002`.

  Declare `mod layout;` in `crates/sesh-cli/src/main.rs`.
- [x] T004 Unit test (written failing): `deploy/compose.yaml` and `deploy/compose.stub.yaml` match `layout.rs`.
  - Every service's `networks:` list and every network's `internal:` flag (`Network.internal`).
  - `stub` appears only in the stub file (`Network.stub_only`).
  - No service uses `default`.

  The test lives in `crates/sesh-cli/src/up.rs` tests, reusing `block` and `members`.

---

## Phase 3: User Story 1 - A compromised service reaches only its own neighbours (Priority: P1) 🎯 MVP

**Goal**: each service reaches only its row of the reachability table.

**Independent test**: `network_stack` probes every (service, destination, port) from inside each service's network namespace.

- [x] T005 [US1] Stack test (written first) `network_stack` in `crates/sesh-cli/tests/stack.rs`, in stub mode with one agent.
  - **Networks:** for every running service, `networks_of` equals the layout table's list (FR-002 to FR-008).
  - **Reachability:** for every (from, to, port), the expected result is computed from `layout.rs` (`Allowed path.kind`).
    - Probes use `probe_from`, run in the source's namespace from the gateway image.
    - Each destination is probed **by every address it has**, as well as by name.
    - Pairs that don't share a network must fail.
    - `used` and `reverse` paths must connect; these are the positive controls.
    - The gateway's `:7000` and `:7002` must fail from every container but itself (FR-009).
  - **Edge networks:** every edge network has one member, and Postgres has no non-internal network.
  - **Published ports:** still loopback only (FR-010).
- [x] T006 [US1] `deploy/compose.yaml`: remove every implicit `default` and declare the networks in contracts/networks.md.
  - The database networks, the pair networks and the edge networks.
  - Postgres joins the six `*-db` networks; each service gets exactly its row.
  - LiteLLM keeps `extra_hosts` for `host.docker.internal` on `litellm-edge` (research.md R3).
  - Comment the purpose of each network as the existing ones do.
- [x] T007 [US1] Executors and Kiwix in the generated overlay (`crates/sesh-cli/src/up.rs` `generated_overlay`): the gateway's network list there gains `gateway-db`, `secrets`, `notify`, `gateway-edge` instead of `default`. Update its unit tests (`networks: [default, search, …]` → the new list).
- [x] T008 [P] [US1] Unit test, then a doc comment in `crates/sesh-gateway/src/listen.rs`: with the route address on the edge network, `agent_addrs` and `addrs_on(7002, …)` give loopback plus that address only, never `0.0.0.0` (research.md R5). No behaviour change.

- [x] T009 [US1] Unit tests first, then `crates/sesh-cli/src/pg_hba.rs`: render the login rules from data-model.md `Database login rule` (`Database login rule.role`, `Database login rule.database`, `Database login rule.network`, `Database login rule.subnets`).
  - **Output:** one `host <database> <role> <subnet> scram-sha-256` line per subnet, in a stable order, then `local all all trust`, then `host all all all reject`. No network line for `sesh`.
  - **Before any subnet is known:** only the local line and the reject.
  - **A role whose subnets are empty or unreadable:** an error naming its network, and no permissive line.
  - The roles and networks come from `layout.rs`.
- [x] T010 [US1] Wire the rules in (research.md R10).
  - **`Compose::network_subnets(network) -> Result<Vec<String>>`**, using `docker network inspect`, in `crates/sesh-cli/src/host/compose.rs`, plus the fake in `host/fake.rs`.
  - **`deploy/compose.yaml`:** Postgres gets `command: ["postgres", "-c", "hba_file=/etc/sesh-pg/pg_hba.conf"]` and mounts `${SESH_DATA_DIR:?}/postgres-conf:/etc/sesh-pg:ro`, a directory mount.
  - **`up.rs`:**
    - `write_compose_files` writes the local-plus-reject file if none exists;
    - `run`, after `up postgres`, reads each `*-db` network's subnets, writes the file atomically in its directory (only when it changed), and runs `SELECT pg_reload_conf()` through `exec_sql`;
    - it prints per contracts/cli.md, and fails closed when a subnet can't be read.
  - **Unit tests:** a first `sesh up` writes and reloads; an unchanged second run does neither; a missing subnet fails.
- [x] T011 [US1] Stack steps in `network_stack` (written before T010 lands). Probes use `docker run --network container:<source>` with the Postgres image's `psql`, taking passwords from `secrets.env` through the environment:
  - each service's login works from its own namespace;
  - from another storing service's namespace, which can reach Postgres on its own database network, that login is rejected with "pg_hba.conf rejects connection";
  - the superuser is rejected over the network from every storing service's namespace;
  - `pg_hba_file_rules` matches the subnets that `docker network inspect` reports (FR-018 to FR-020, SC-007).

---

## Phase 4: User Story 2 - Everything keeps working (Priority: P1)

**Goal**: every existing path the household uses still works.

**Independent test**: the seven existing stack tests pass on the new layout.

- [x] T012 [US2] Review each existing stack test for `sesh_default` or `["default"]` assumptions and update them to the new networks:
  - `foundation_stack`'s port and network checks;
  - `gateway_stack`'s executor probes (`1.1.1.1` and `gateway:7000` from an executor);
  - `feeds_stack`'s `default_ip` helper, which reads `sesh_default`: use `gateway-db` and `gateway-edge` for the gateway and the `*-db` networks for Postgres;
  - `isolation_stack`'s Redis and Postgres probes.

  All in `crates/sesh-cli/tests/stack.rs`.
- [x] T013 [US2] `crates/sesh-cli/src/doctor.rs`: make sure existing checks don't depend on `default`. `ports-loopback` reads published ports, and `services-healthy` reads states. Update the fixtures in its tests if they name `default`.

---

## Phase 5: User Story 3 - The upgrade is just `sesh up` (Priority: P1)

**Goal**: an existing install moves to the new layout with `sesh up`.

**Independent test**: a pre-008 stack with data, then the new `sesh up`; the data is intact and `sesh_default` is gone.

- [x] T014 [US3] Unit tests, then the code.
  - `Compose::remove_network_if_unused(name) -> Result<Option<Vec<String>>>`, returning the containers still attached, if any. Implemented with `docker network inspect` and `docker network rm` in `crates/sesh-cli/src/host/compose.rs`, plus the fake in `host/fake.rs`.
  - `up::run` calls it for `sesh_default` after the services are up and reports per contracts/cli.md: removed, still in use, or absent (silent).
- [ ] T015 [US3] Stack steps in `network_stack`:
  - Rewind to the pre-008 layout: a fixture override that puts every service back on `default`, in `crates/sesh-cli/tests/fixtures/pre-008.compose.yaml`.
  - Create data: an Open WebUI account, a router key, a credential and a document tag, reusing `isolation_stack`'s helpers.
  - Run the new `sesh up`.
  - The data is there, `sesh_default` is gone, and the layout and reachability checks pass.
  - A second `sesh up` prints nothing new (FR-012).

---

## Phase 6: User Story 4 - Development and evals still need no internet (Priority: P2)

**Goal**: stub mode and the eval project keep working with no internet.

**Independent test**: the stub-mode stack tests and `eval_stack` pass with containers' internet blocked.

- [x] T016 [US4] `deploy/compose.stub.yaml`: an internal `stub` network (`Network.stub_only`) joining `stub-llm`, `litellm` and `gateway`. `stub-llm` moves from `default` to `[stub, feeds-egress]`. `network_stack` checks that LiteLLM, the gateway and Miniflux reach `stub-llm:8000`.
- [ ] T017 [P] [US4] Confirm the eval project (`deploy/eval/compose.yaml`) is unaffected. It has no `default` and no reference to household networks. Add a unit test in `crates/sesh-cli/src/eval/mod.rs` asserting it never names a household network.

---

## Phase 7: User Story 5 - The operator can see the layout is intact (Priority: P3)

**Goal**: the doctor reports layout drift.

**Independent test**: the doctor passes on the intended layout, and fails, naming the service and the network, when a container is attached to an extra network.

- [x] T018 [US5] Unit tests, then the `network-layout` check in `crates/sesh-cli/src/doctor.rs`, per contracts/cli.md.
  - It reads each running sesh container's networks through `Compose` and compares them with `layout.rs` (`Network.members`). It also checks whether `sesh_default` exists. It also checks that `pg_hba_file_rules` matches the current subnets: each login only on its own network, no network rule for `sesh`, and a final reject (contracts/cli.md).
  - The fake compose in `host/fake.rs` gains network data.
- [ ] T019 [US5] Stack step in `network_stack`: `docker network connect sesh_secrets <open-webui>`. The doctor fails, naming `open-webui` and `secrets`. Then disconnect, and the doctor passes again.

---

## Phase 8: Polish

- [ ] T020 [P] Architecture doc: a network-layout section (the networks and the reachability rule), and updates to the deployment table and the security section. `diagram.jsx` and `docs/architecture.svg`: update them if the diagram shows networks or shared links; the diagram check in `make check` keeps them identical.
- [x] T021 [P] `.github/workflows/ci.yml`: add `network_stack` to the lightest stack group.
- [ ] T022 Record CI results in `specs/008-network-isolation/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → US5 → Polish.
- T003 blocks T004, T005, T015 and T018, since all read `layout.rs`.
- T005 is written before T006 and T007 (it fails until they land).
- T006, T007 and T016 together make the stub-mode stack start.
- T012 depends on T006.
- T009 blocks T010; T011 is written before T010 lands.

## Parallel opportunities

- T001 and T002 (docs).
- T008 (the gateway's listener test) alongside T005 to T007.
- T017 (the eval check) alongside T016.
- T020 and T021 (polish).

## Implementation strategy

The MVP is US1 together with US2: the new layout, the reachability test, and every existing test still passing. Then US3 (the upgrade), US4 (stub and eval), and US5 (the doctor).
