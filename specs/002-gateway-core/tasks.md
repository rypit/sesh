---
description: "Task list for feature 002: gateway core"
---

# Tasks: Gateway core

**Input**: `specs/002-gateway-core/` (plan, spec, research, data-model, contracts)

**Tests**: Required (constitution X; FR-017). Unit tests come before the
code in each task; stack tests are written failing before the stack work
they check.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [x] T001 Add crates to the workspace in `Cargo.toml`: `crates/sesh-core`, `crates/sesh-gateway`, `crates/sesh-executor-supervisor` (stubs that compile), with workspace dependencies from research.md R8
- [x] T002 [P] Executor project in `executor/`: `package.json` pinning `@cloudflare/codemode` 0.5.6, `workerd` and `esbuild`; `package-lock.json`; `workerd.capnp`; empty `src/host.js`
- [x] T003 [P] Images: `deploy/gateway.Dockerfile` and `deploy/executor.Dockerfile` (bundle with esbuild, workerd binary, supervisor binary); CI builds both in the stack job (`.github/workflows/ci.yml`)

---

## Phase 2: Foundational

- [x] T004 [P] Unit tests, then shared types in `crates/sesh-core/src/lib.rs` per data-model.md Agent (`Agent.name`, `Agent.token_sha256`, `Agent.connectors`) and Connector descriptor (`Connector descriptor.name`, `Connector descriptor.instructions`, `Connector descriptor.methods.<name>.description`, `Connector descriptor.methods.<name>.input`, `Connector descriptor.methods.<name>.requires_approval`), plus `agents.toml` load/save
- [x] T005 [P] Unit tests, then `crates/sesh-gateway/src/schema.rs`: validator for the schema subset and TypeScript declaration generation (research.md R7)
- [x] T006 [P] Unit tests, then `crates/sesh-gateway/src/redact.rs`: argument summaries per data-model.md Audit entry (`Audit entry.args_summary`)
- [x] T007 [P] Unit tests, then `crates/sesh-gateway/src/capability.rs`: issue, check (agent, execution, expiry), revoke, per data-model.md Execution (`Execution.id`, `Execution.capability`, `Execution.expires_at`, `Execution.calls`)
- [x] T008 Audit seam `crates/sesh-gateway/src/audit.rs` (trait + Postgres implementation + in-memory fake) per data-model.md Audit entry; `deploy/postgres/init.sql` and the idempotent migration SQL (database `gateway`, table, role `sesh_gateway` with INSERT-only grants)
- [x] T009 Settings in `crates/sesh-cli/src/config.rs` and gateway: `[gateway]` per data-model.md Gateway settings (`deadline_secs`, `max_calls`, `max_result_bytes`, `home.latitude`, `home.longitude`, `upstreams.*`); `sesh init --home`

---

## Phase 3: User Story 1 - Work done by one script (P1) 🎯 MVP

- [x] T010 [P] [US1] Unit tests with fake upstreams, then connectors in `crates/sesh-gateway/src/connectors/search.rs` and `crates/sesh-gateway/src/connectors/geo.rs` per contracts/connectors.md
- [x] T011 [P] [US1] Fake upstream routes (SearXNG, Kiwix, Open-Meteo, Nominatim shapes) in `crates/sesh-stub-llm/src/main.rs`
- [x] T012 [US1] Executor-facing `/call` in `crates/sesh-gateway/src/calls.rs`: capability check, allowlist, approval refusal, input validation, call limit, dispatch, audit (refuse when the audit write fails); unit tests for each refusal
- [x] T013 [US1] Executor host Worker in `executor/src/host.js` and `executor/workerd.capnp` per contracts/executor.md; supervisor in `crates/sesh-executor-supervisor/src/main.rs` (spawn workerd, proxy `/execute` with deadline, kill and restart on overrun, 504)
- [x] T014 [US1] MCP server in `crates/sesh-gateway/src/mcp.rs`, `auth.rs`, `executor.rs`, `main.rs` per contracts/mcp.md: token middleware, `execute` (capability, executor call with deadline, truncation, untrusted label), `search`, `describe`; two listeners
- [x] T015 [US1] Stack test (written first, seen failing) in `crates/sesh-cli/tests/stack.rs`: a test MCP client lists tools, searches, describes, and executes a script calling `search.web` and `geo.forecast` in one execution (SC-001)

## Phase 4: User Story 2 - No escape, no widening (P1)

- [x] T016 [US2] Stack tests in `crates/sesh-cli/tests/stack.rs`: script finds no env, files or network; unknown token 401; ungranted connector refused; forged, expired and other-agent capabilities refused on `:7001`; agent token refused on `:7001` and capability on `:7000`; `testgate.act` refused even with `approved` claimed; scope arguments ignored; executor network can't reach searxng
- [x] T017 [US2] Compose rendering in `crates/sesh-cli/src/up.rs` and `deploy/compose.yaml`: gateway, searxng (`deploy/searxng/settings.yml`), optional kiwix profile, per-agent `compose/agents.yaml` with internal networks; gateway publishes only `127.0.0.1:7000`

## Phase 5: User Story 3 - On the record (P1)

- [x] T018 [US3] Stack tests: two calls → two audit rows with redacted summaries; refused calls have reasons; `sesh_gateway` can't SELECT, UPDATE or DELETE `audit`; no token or capability appears in audit rows or gateway logs (SC-003, SC-006)
- [x] T019 [US3] Migration step in `crates/sesh-cli/src/up.rs` (compose exec psql as owner) and the `audit-refusals` doctor check in `crates/sesh-cli/src/doctor.rs`

## Phase 6: User Story 4 - Runaway isolation (P2)

- [x] T020 [US4] Stack test: agent A runs `while(true){}`; agent B's script finishes within 2 s; A gets `timeout` within deadline + 5 s; A's next script works (SC-004)

## Phase 7: User Story 5 - Operator manages agents (P2)

- [x] T021 [US5] Unit tests, then `sesh agent add/remove/list` in `crates/sesh-cli/src/agent.rs` per contracts/cli.md; gateway reloads `agents.toml` on change
- [x] T022 [US5] Doctor checks `gateway-healthy` and `executors-healthy` in `crates/sesh-cli/src/doctor.rs`; stack test: removed agent's token is refused

## Phase 8: Polish

- [x] T023 [P] Update `README.md`, `AGENTS.md` (images to build), quickstart; architecture diagram unchanged (no component moved)
- [x] T024 Record CI results for SC-001–SC-006 in `specs/002-gateway-core/research.md`

## Dependencies

- Setup → Foundational → US1 → (US2, US3, US4, US5 in parallel; all extend the stack test file, so their test edits run in sequence).
- T013 needs T002; T014 needs T005, T007, T012, T013; T017 needs T014.

## Implementation Strategy

MVP = US1 (a script reaching two connectors through the gateway). Then
the negative tests (US2, US3), isolation (US4), and operator commands
(US5).
