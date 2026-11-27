---
description: "Task list for feature 001: foundation stack"
---

# Tasks: Foundation stack

**Input**: Design documents from `specs/001-foundation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Required. The constitution (Principle X, Development Workflow)
requires test-first and negative tests for each isolation rule; FR-012
lists them. Unit tests come before the code in each task; stack tests
are written failing before the stack work they check.

**Organization**: Tasks are grouped by user story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependencies)
- **[Story]**: user story (US1–US5)

---

## Phase 1: Setup

- [X] T001 Create the Cargo workspace: `Cargo.toml` (members `crates/sesh-cli`, `crates/sesh-stub-llm`; workspace lints with `unsafe_code = "forbid"`), `rust-toolchain.toml` (stable, pinned), `crates/sesh-cli/src/main.rs` and `crates/sesh-stub-llm/src/main.rs` stubs that compile
- [X] T002 [P] Add `deny.toml` (licences, advisories, duplicate versions) and a `Makefile` with `check` (cargo fmt --check, clippy -D warnings, cargo test, cargo deny check, `python3 scripts/render-architecture.py --check`, `docker compose -f deploy/compose.yaml -f deploy/compose.stub.yaml config -q`) and `stack-test` targets in `Makefile`
- [X] T003 [P] Extend `.gitignore` with `target/`, `*.env`, and any local `sesh-data/`; add a `make check` step that fails if a file named `secrets.env` or `litellm.yaml` is tracked, in `Makefile`

---

## Phase 2: Foundational (blocks all stories)

- [X] T004 [P] Unit tests, then the config model in `crates/sesh-cli/src/config.rs` per data-model.md `Config.inference.base_url`, `Config.inference.api_key_env`, `Config.inference.timeout_secs`, `Config.aliases.<alias>`, `Config.ports.webui`, `Config.ports.router` (serde `deny_unknown_fields`; load and save `sesh.toml`; `agent-main` required)
- [X] T005 [P] Unit tests, then secrets handling in `crates/sesh-cli/src/secrets.rs` per data-model.md Secrets: generate missing values with `getrandom` (32 bytes, URL-safe base64), write `secrets.env` with mode 0600 and dir 0700, never print values; the `Debug` impl redacts
- [X] T006 [P] Unit tests, then the pure router-config renderer in `crates/sesh-cli/src/render.rs` per data-model.md Rendered router config and contracts/config.md `litellm.yaml` (one entry per alias, `openai/<model>`, no wildcards, telemetry off)
- [X] T007 [P] Seam `crates/sesh-cli/src/host/compose.rs`: a trait plus the only implementation running `docker compose` (up, down, ps as JSON with publishers and health, exec), and a fake in `crates/sesh-cli/src/host/fake.rs` for tests
- [X] T008 [P] Seam `crates/sesh-cli/src/host/http.rs`: trait + `ureq` implementation for JSON GET/POST with bearer key and timeout, plus a fake
- [X] T009 [P] Stub server in `crates/sesh-stub-llm/src/main.rs` per research.md R6 (`/v1/models` lists `stub-model` and `stub-raw-2`; chat replies "stub reply", SSE when `stream: true`; with tools, a call to the first tool), with unit tests; `deploy/stub.Dockerfile` builds it
- [X] T010 Compose files per contracts/stack.md: `deploy/compose.yaml` (postgres, redis, litellm, open-webui; loopback-only published ports from `${SESH_WEBUI_PORT}` and `${SESH_ROUTER_PORT}`; `env_file: ${SESH_DATA_DIR}/secrets.env`; LiteLLM mounts `${SESH_DATA_DIR}/litellm.yaml`; health checks; named volumes), `deploy/compose.stub.yaml` (adds `stub-llm`), `deploy/postgres/init.sql` (databases `openwebui`, `litellm`)
- [X] T011 `sesh init` in `crates/sesh-cli/src/init.rs` per contracts/cli.md: create `$SESH_DATA_DIR` (default `~/.local/share/sesh`), write or update `sesh.toml` from flags, generate secrets, set `WEBUI_ADMIN_EMAIL`, render `litellm.yaml`; idempotent; tests with a temp dir

**Checkpoint**: config, secrets, rendering, seams, stub and Compose files exist and are unit-tested.

---

## Phase 3: User Story 1 - Chat over the tailnet (Priority: P1) 🎯 MVP

**Goal**: a member chats with `agent-main` in Open WebUI; nothing is reachable from the LAN.

**Independent Test**: spec.md US1.

- [X] T012 [US1] Stack test harness in `crates/sesh-cli/tests/stack.rs` (`#[ignore]`): temp `SESH_DATA_DIR`, `sesh init`, `sesh up --stub`, teardown with `sesh down` and volume removal; first tests: Open WebUI health, admin sign-in via its API with the generated password, one chat to `agent-main` returns "stub reply" — written before T013 and seen failing
- [X] T013 [US1] Negative stack test in `crates/sesh-cli/tests/stack.rs`: every published port of every service is bound to 127.0.0.1 (from `docker compose ps`), and connecting to the host's non-loopback address on those ports fails
- [X] T014 [US1] `sesh up` / `sesh down` in `crates/sesh-cli/src/up.rs` per contracts/cli.md: re-render config, start postgres, redis, litellm, wait for health, start open-webui, wait for health; `--stub` points the inference setting at `http://stub-llm:8000/v1` with alias `agent-main = stub-model`
- [X] T015 [US1] Stack test: with the stub stopped, a chat returns an error promptly and works again after restarting the stub, in `crates/sesh-cli/tests/stack.rs`

**Checkpoint**: chat works end to end against the stub; ports are loopback-only.

---

## Phase 4: User Story 2 - Members see only sanctioned models (Priority: P1)

**Goal**: non-admins see only `agent-main`; the admin can't read their chats; built-in memory is off.

**Independent Test**: spec.md US2.

- [X] T016 [US2] Negative stack tests in `crates/sesh-cli/tests/stack.rs`: the admin creates a non-admin user; that user's model list is exactly `["agent-main"]`; the user creates a chat; the admin's API request for that chat is refused; memory endpoints are disabled — written first, seen failing
- [X] T017 [US2] Open WebUI environment in `deploy/compose.yaml` per research.md R1 and contracts/stack.md (persistent config off, memories off, admin chat access off, sign-up off, task model, Ollama off, Postgres, Redis, offline and telemetry settings, `BYPASS_MODEL_ACCESS_CONTROL=True`) until T016 passes

---

## Phase 5: User Story 3 - Any inference server (Priority: P1)

**Goal**: one setting group selects the inference server; raw names are refused.

**Independent Test**: spec.md US3.

- [X] T018 [US3] Negative stack tests in `crates/sesh-cli/tests/stack.rs`: with Open WebUI's router key, `stub-raw-2` and `openai/stub-model` are refused; with no key, 401; switching `agent-main` to `stub-raw-2` via `sesh init --model stub-raw-2` and `sesh up` changes only `litellm.yaml` and restarts only `litellm`
- [X] T019 [US3] Restricted client key in `crates/sesh-cli/src/up.rs`: on first run, create a LiteLLM virtual key limited to the configured aliases using the master key, store it as `WEBUI_ROUTER_KEY` in `secrets.env` (data-model.md Secrets), pass it to Open WebUI; if T018 shows `openai/<model>` still accepted, record that in `specs/001-foundation/research.md` R2 and keep the test as a documented known risk
- [X] T020 [US3] `sesh up` restarts only the services whose config changed (rendered `litellm.yaml` hash) in `crates/sesh-cli/src/up.rs`

---

## Phase 6: User Story 4 - `sesh doctor` (Priority: P2)

**Goal**: one command reports health and safety with fixes.

**Independent Test**: spec.md US4.

- [X] T021 [P] [US4] Seam `crates/sesh-cli/src/host/tailscale.rs`: trait + implementation of `tailscale serve status --json`, returning whether the Open WebUI port is served; fake for tests
- [X] T022 [US4] Unit tests with fakes, then `crates/sesh-cli/src/doctor.rs` per contracts/cli.md checks and data-model.md Doctor check (`Doctor check.id`, `Doctor check.status`, `Doctor check.detail`, `Doctor check.fix`): `alias-answers`, `alias-tools`, `ports-loopback`, `services-healthy`, `tailscale-serve` (warn only); text and `--json` output; exit codes
- [X] T023 [US4] Stack tests in `crates/sesh-cli/tests/stack.rs`: healthy stack → exit 0; stub stopped → `alias-answers` fails; a port overridden to `0.0.0.0` → `ports-loopback` fails naming the service; stub run with tool calls disabled (`STUB_TOOLS=off`) → `alias-tools` fails; completes in under 30 s (SC-005)

---

## Phase 7: User Story 5 - Build and test without a model (Priority: P2)

**Goal**: CI runs everything with the stub.

**Independent Test**: spec.md US5.

- [X] T024 [US5] `.github/workflows/ci.yml`: job `check` (`make check`) and job `stack` (build the stub image, `make stack-test`, upload `docker compose logs` on failure); remove `.github/workflows/docs.yml`, now covered by `make check`
- [X] T025 [US5] Pin images by digest in `deploy/compose.yaml` once CI resolves them, and record the digests in `specs/001-foundation/contracts/stack.md`

---

## Phase 8: Polish

- [X] T026 [P] Update `README.md` (getting started with the stub, `sesh init/up/doctor`) and `AGENTS.md` (commands now defined: `make check`, `make stack-test`)
- [X] T027 [P] Add a Tailscale exposure note and the inference-switching steps to `specs/001-foundation/quickstart.md` as validated
- [X] T028 Run `specs/001-foundation/quickstart.md` "Without a real model" end to end in CI and record SC-002, SC-003, SC-005, SC-006 results in `specs/001-foundation/research.md`

---

## Dependencies & Execution Order

- Setup → Foundational → stories. T010 needs T009 (stub image); T011 needs T004–T006.
- US1 (T012–T015) first: it builds `sesh up` and the stack harness the other stories use.
- US2 and US3 can run in parallel after US1; T019 changes `up.rs`, so it follows T014.
- US4 needs US1's stack; its unit work (T021, T022) can start after Foundational.
- US5 can start any time after T002; its stack job needs US1.
- Polish last.

## Parallel Example: Foundational

```text
T004 config.rs · T005 secrets.rs · T006 render.rs · T007 host/compose.rs · T008 host/http.rs · T009 stub server
```

## Implementation Strategy

1. Setup + Foundational, then US1: the stack chats against the stub with loopback-only ports. **MVP.**
2. US2 and US3: privacy settings and the alias rules, each with negative tests.
3. US4: `sesh doctor`.
4. US5: CI runs it all; digests pinned.
5. Validate the quickstart with a real inference server when the operator is at the Mac.
