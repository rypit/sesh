---
description: "Task list for feature 009: one agent end to end"
---

# Tasks: One agent end to end

**Input**: `specs/009-one-agent/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; spec FR-012). Each unit test is written failing before its code; `agent_stack` is written before the Compose changes it checks.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [x] T001 [P] ADR `docs/decisions/0012-agent-runtime.md` (accepted): the runtime, the tools, the networks, the listener change, the credentials (research.md R1–R9).
- [x] T002 [P] Pin the image in `crates/sesh-cli/src/hermes.rs` (`HERMES_IMAGE`, research.md R1) (`IMAGE`); Compose pulls it, in CI too.

---

## Phase 2: Foundational

- [x] T003 `Agent.runtime: Option<String>` (`#[serde(default, skip_serializing_if)]`) in `crates/sesh-core/src/lib.rs`; a unit test that an old `agents.toml` still loads and the gateway ignores the field.
- [x] T004 Unit tests, then `crates/sesh-cli/src/agent.rs`: `--runtime hermes` (unknown runtime → error per contracts/cli.md); creates `AGENT_<NAME>_API_KEY` in `secrets.env`; `remove` deletes it and the router key's secret; `list` shows the runtime. Wire the flag in `main.rs`.
- [x] T005 Unit tests, then `crates/sesh-cli/src/hermes.rs`:
  - `config_yaml(agent)`: `platform_toolsets.api_server: [mcp-sesh]`; `agent.disabled_toolsets` = every name in `hermes/toolsets.txt`; `mcp_servers.sesh` with `http://gateway:7000/mcp` and `Authorization: Bearer ${SESH_GATEWAY_TOKEN}`; the model (`custom`, `http://litellm:4000/v1`, `agent-main`, `${SESH_ROUTER_KEY}`) and every auxiliary task on `main`; memory off. Tests: no secret value in it; every built-in toolset disabled; `mcp-sesh` is the only platform toolset.
  - `env_file(token, router_key, api_key)`: the three variables, written 0600.
  - `service(agent)`: the Compose block per data-model.md (image, `env_file`, volume, read-only config mount, three networks, `API_SERVER_*` settings without values, healthcheck on `/health`, hardening per R9).
- [x] T006 Unit tests, then `crates/sesh-cli/src/layout.rs`: `agent_nets(name)` and `matrix_with(stub, agents)` (data-model.md); the rules hold with agents (each agent network is a pair, internal; the runtime has no non-internal network; `gateway:7000` reachable only from the agent's own runtime; `gateway:7002` from none).

---

## Phase 3: User Story 3 - No way out except the gateway (Priority: P1)

Written first: it's the safety guarantee, and its tests define the wiring the other stories use.

- [x] T007 [US3] Unit tests, then `crates/sesh-gateway/src/listen.rs` and `main.rs`: `agent_listen` in the gateway settings; each alias resolved (retrying for up to 30 s) and `:7000` bound on its address; an unresolvable alias is a start-up error; the addresses never include `0.0.0.0`; `:7002` unchanged.
- [x] T008 [US3] Unit tests, then `crates/sesh-cli/src/up.rs` `generated_overlay`: for an agent with a runtime, the `hermes-<name>` service, the volume, the three networks; the gateway joins `agent-<name>` with the alias `gateway-agent-<name>` (map syntax, merged with the existing list); LiteLLM joins `agent-<name>-model`; Open WebUI joins `agent-<name>-chat`. An agent without a runtime gets none of this. `gateway.toml`'s `agent_listen` lists the aliases.
- [ ] T009 [US3] Stack test (written first) `agent_stack` part 1 in `crates/sesh-cli/tests/stack.rs`, stub mode with `STUB_SCRIPT=agent` and the Household agent with a runtime:
  - the stub's recorded requests from the runtime offer only tools ending in `search` or `execute` (SC-002);
  - `[agent:terminal]`: no command runs (the reply says the tool isn't available) and the audit log has no new entry;
  - reachability from the runtime's namespace (`probe_from`): `gateway:7000`, `litellm:4000` connect; the internet (`1.1.1.1:443`), Postgres, Infisical, ntfy, Redis, SearXNG, Paperless, Miniflux, the gateway's `:7002`, an executor and the stub fail (SC-003);
  - the container's environment holds only `SESH_GATEWAY_TOKEN`, `SESH_ROUTER_KEY`, `API_SERVER_KEY` as secrets (US3 scenario 4).
- [x] T010 [US3] The layout and the matrix with a runtime agent (`matrix_with`): checked in `agent_stack` (`assert_runtime_reachability`) rather than `network_stack`, so only one CI group pulls the runtime's image.

---

## Phase 4: User Story 1 - Chat in Open WebUI (Priority: P1) 🎯 MVP

- [x] T011 [US1] Unit tests, then `crates/sesh-stub-llm/src/main.rs`: `STUB_SCRIPT=agent` per contracts/stub-script.md and `GET /stub/requests`.
- [x] T012 [US1] Unit tests, then `up.rs`: Open WebUI's `OPENAI_API_BASE_URLS`/`OPENAI_API_KEYS` (router first, then each runtime, keys interpolated from `secrets.env`) in the overlay; `deploy/compose.yaml` moves Open WebUI to the list form. The router key per runtime agent is created/updated like Open WebUI's, and deleted for a removed agent. `sesh up` writes the config and the env file (only when changed), and starts the runtime after the gateway and LiteLLM and before Open WebUI.
- [ ] T013 [US1] `agent_stack` part 2: sign in to Open WebUI as the admin, list models (`household-agent` present), chat `[agent:weather]` and `[agent:search]` with streaming; the answers quote the fake forecast and search result; the audit log has a `geo.forecast` and a `search.web` call by `household-agent` (SC-001).

---

## Phase 5: User Story 2 - The household's accounts (Priority: P1)

- [ ] T014 [US2] `agent_stack` part 3: with fake Google connected for `household` and a fake feed followed, chat `[agent:mail]` and `[agent:feeds]` (answers quote the fakes); `[agent:send]` produces exactly one approval request on the Household topic and the fake Gmail records no send until it's approved (SC-004).

---

## Phase 6: User Story 4 - One command to add or remove (Priority: P2)

- [x] T015 [US4] Unit tests, then `doctor.rs` `agent-runtimes` (contracts/cli.md): health, `/v1/toolsets` (no built-in toolset enabled; research.md R12), config matches. `network-layout` includes the agent networks.
- [ ] T016 [US4] `agent_stack` part 4: doctor passes; `sesh agent remove household-agent` and `sesh up`: the container and its networks are gone, Open WebUI no longer lists it, its gateway token is refused and its router key is deleted; doctor passes (SC-005).

---

## Phase 7: Polish

- [x] T017 [P] Architecture doc (the runtime, its networks, the listener rule, phase 9 done) and `diagram.jsx` + `docs/architecture.svg` (Hermes per agent, its three links); README status; AGENTS.md (the runtime flag).
- [x] T018 [P] `.github/workflows/ci.yml`: `agent_stack` in its own stack group.
- [x] T019 Record CI results and any deviations (R8's read-only config, R9's hardening) in `research.md`.

## Dependencies

- Setup → Foundational → US3 → US1 → US2 → US4 → Polish.
- T005 and T006 block T008; T007 and T008 block T009's passing.
- T011 and T012 block T013 and T014.

## Parallel opportunities

- T001 and T002; T003 alongside T005 and T006; T017 and T018.

## Implementation strategy

The MVP is US3 with US1: a runtime that can only reach the gateway,
answering in Open WebUI. Then the household's accounts, then the
operator's add/remove and the doctor.
