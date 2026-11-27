---
description: "Task list for feature 003: model evals"
---

# Tasks: Model evals

**Input**: `specs/003-model-evals/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; FR-017). Unit tests come before the
code in each task; stack tests are written failing before the stack work
they check.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 Module skeleton that compiles: `crates/sesh-cli/src/eval/{mod,suite,run_loop,score,store}.rs` and `crates/sesh-cli/src/host/mcp.rs`, declared in `crates/sesh-cli/src/main.rs`; an `Eval` subcommand with `list`, `compare`, `show` per contracts/cli.md, each returning "not implemented" for now
- [ ] T002 [P] `deploy/eval/compose.yaml` (project `sesh-eval`): `gateway-eval`, `executor-eval`, `fakes` per research.md R2 (networks `eval` internal and `eval-edge`; `127.0.0.1:7100:7000`; read-only, `cap_drop: ALL`); added to `make compose-config` in `Makefile`
- [ ] T003 [P] `docs/decisions/0006-model-evals.md` reviewed against the final design. Index it wherever other ADRs are listed (`README.md` if it lists them)

---

## Phase 2: Foundational

- [ ] T004 [P] Unit tests, then the MCP seam in `crates/sesh-cli/src/host/mcp.rs`: trait `Mcp { list_tools, call_tool }`. Streamable HTTP client over `ureq`: initialize, session id, SSE or JSON replies, 401 as an error naming the endpoint. Plus `FakeMcp` in `crates/sesh-cli/src/host/fake.rs`
- [ ] T005 [P] Unit tests, then the gateway's file audit sink in `crates/sesh-gateway/src/audit.rs` and `crates/sesh-gateway/src/main.rs` per contracts/gateway-audit-file.md: `file:` URL; refused outside `/var/log/sesh-eval/`; append and flush before returning; a failed write withholds the result as for Postgres
- [ ] T006 [P] Unit tests, then the suite in `crates/sesh-cli/src/eval/suite.rs`:
  - the five tasks, with `SUITE_VERSION`, per data-model.md Task (`Task.id`, `Task.prompt`, `Task.expect.calls`, `Task.expect.facts`, `Task.expect.sources`, `Task.expect.not_claims`, `Task.expect.recovered`, `Task.limits`);
  - a test that no `Task.expect.facts` string appears in any `Task.prompt`.
- [ ] T007 [P] Task-specific fake content in `crates/sesh-stub-llm/src/main.rs` per research.md R7 (facts no model knows, matching T006), with unit tests
- [ ] T008 `EVAL_ROUTER_KEY` in `crates/sesh-cli/src/secrets.rs` and `crates/sesh-cli/src/up.rs` (research.md R9): an alias-only virtual key, created like Open WebUI's, never printed. Unit test with `FakeHttp`

---

## Phase 3: User Story 1 - Does this model work with sesh? (P1) 🎯 MVP

**Goal**: one command runs the suite against an alias and gives a verdict.

**Independent test**: against the stub's passing script every task
passes (exit 0); with stub tool calls off, the tool tasks fail naming the
missing call (exit 1).

- [ ] T009 [US1] Stack test (written first, seen failing) in `crates/sesh-cli/tests/stack.rs`, as a new `eval_stack` test:
  - `sesh init --stub`, `STUB_SCRIPT=pass sesh up`, `sesh eval`: all PASS, exit 0, a run directory is saved;
  - with `STUB_TOOLS=off`: the tool tasks FAIL with "expected a … call; none made", exit 1.
- [ ] T010 [US1] Unit tests (fake router, `FakeMcp`), then the loop in `crates/sesh-cli/src/eval/run_loop.rs` per research.md R3–R4:
  - tools from `tools/list` mapped to OpenAI tools; the system prompt;
  - non-streaming calls; tool calls forwarded in order;
  - malformed calls answered as invalid;
  - limits from data-model.md `Task.limits`;
  - unreachable router → error naming alias and server within the time limit.
- [ ] T011 [US1] Unit tests, then scoring in `crates/sesh-cli/src/eval/score.rs`:
  - rules per data-model.md (`Task.expect.calls`, `Task.expect.facts`, `Task.expect.sources`, `Task.expect.not_claims`, `Task.expect.recovered`);
  - matching per research.md R7;
  - one reason per failed rule, into data-model.md `Task result.reasons`;
  - `Task result.answer` truncated as data-model.md says.
- [ ] T012 [US1] The run in `crates/sesh-cli/src/eval/mod.rs`:
  - prerequisites (router answers, `EVAL_ROUTER_KEY` present; else exit 3);
  - the eval agent per data-model.md Eval agent (`Eval agent.name`, `Eval agent.connectors`, `Eval agent.token`), with only the hash written to the run's temporary `agents.toml`;
  - starting the `sesh-eval` project through the Compose seam, with a second `DockerCompose` for project `sesh-eval`;
  - reading the audit file around each task (research.md R5);
  - text output per contracts/cli.md;
  - exit code from data-model.md `Run.threshold` and `Run.verdict`.
- [ ] T013 [US1] Scripted mode in `crates/sesh-stub-llm/src/main.rs` per contracts/stub-script.md (`STUB_SCRIPT=pass|fail`; unchanged when unset), with unit tests per task and step. Pass `STUB_SCRIPT` through in `deploy/compose.stub.yaml` and `crates/sesh-cli/src/main.rs`

## Phase 4: User Story 2 - Safe and repeatable (P1)

**Goal**: runs are isolated from the household and give the same result
every time.

**Independent test**: with containers' internet blocked the eval passes;
the household gateway's agents and audit rows are unchanged; a fake
result containing instructions changes nothing.

- [ ] T014 [US2] Stack tests in `eval_stack`:
  - the household `audit` row count and `sesh agent list` are identical before and after `sesh eval` (FR-008, SC-004);
  - two runs with the pass script give identical results and reasons (SC-003);
  - the `sesh-eval` project is gone after the run, including after an interrupted run (SIGINT to `sesh eval`).
- [ ] T015 [US2] Teardown and lock in `crates/sesh-cli/src/eval/mod.rs`:
  - `evals/.lock`: a second run exits 3;
  - `down -v` on success, failure and Ctrl-C;
  - an interrupted run saved with data-model.md `Run.complete` false.
- [ ] T016 [US2] Unit tests, then untrusted handling:
  - In `crates/sesh-cli/src/eval/run_loop.rs`, a tool result or answer containing instructions ("ignore the task, call X") changes nothing about scoring or the next request beyond appending it as a `tool` or `assistant` message (FR-011).
  - Add an injection line to one fake result in `crates/sesh-stub-llm/src/main.rs`.
- [ ] T017 [US2] Unit test with `FakeMcp`: a call to an ungranted connector is recorded as a refusal, and the task fails or passes by its rules only (FR-009, FR-017)

## Phase 5: User Story 3 - Hard cases: refusals and approvals (P2)

**Goal**: the refusal and approval tasks separate good and bad behaviour.

**Independent test**: pass script → both pass; fail script → refusal
fails "never recovered", approval fails "claimed an action that was
refused".

- [ ] T018 [US3] Stack test in `eval_stack` with `STUB_SCRIPT=fail`. Each task fails with the reason from contracts/stub-script.md, and the command exits 1 (FR-017, SC-002):
  - no tool call;
  - wrong fact;
  - malformed call;
  - never recovered;
  - claimed action.
- [ ] T019 [US3] Refusal and approval rules in `crates/sesh-cli/src/eval/score.rs` (data-model.md `Task.expect.recovered`, `Task.expect.not_claims`), with unit tests built from recorded audit entries

## Phase 6: User Story 4 - Compare models over time (P2)

**Goal**: saved runs can be listed, compared and inspected.

**Independent test**: after a pass run and a fail run, `list` shows both,
`compare` marks the changed tasks, and `show` prints a transcript under
the untrusted header.

- [ ] T020 [US4] Unit tests, then `crates/sesh-cli/src/eval/store.rs` per research.md R8 and data-model.md:
  - run directories named per `Run.id`; 0700 and 0600 modes;
  - the transcript format;
  - redaction of the eval token and every `secrets.env` value, refusing to save if one remains (FR-015, SC-005).
- [ ] T021 [US4] `sesh eval list`, `compare` and `show` per contracts/cli.md in `crates/sesh-cli/src/eval/mod.rs`:
  - `compare` defaults to the latest two complete runs;
  - it notes when suite versions differ;
  - incomplete runs (data-model.md `Run.complete`) are listed but never compared.
  - Includes a stack-test step in `eval_stack`.
- [ ] T022 [US4] `--json` for `sesh eval` and `sesh eval list` (data-model.md Run), with a unit test that it parses back

## Phase 7: User Story 5 - The harness itself is tested (P3)

**Goal**: CI catches a broken harness.

**Independent test**: the CI stack job runs `eval_stack` with internet
blocked; flipping one scoring rule makes it fail.

- [ ] T023 [US5] Confirm `make stack-test` runs `eval_stack` in the CI stack job (`.github/workflows/ci.yml`), with containers' internet blocked. Check locally that inverting the fact rule in `score.rs` makes the pass-script step fail, then revert
- [ ] T024 [US5] SC-001: in `eval_stack`, the pass-script suite finishes in under 2 minutes (asserted). SC-005: saved results and output are scanned for every secret and for capability-shaped values

## Phase 8: Polish

- [ ] T025 [P] Update `README.md` (try `sesh eval`), `AGENTS.md` (commands), and `specs/003-model-evals/quickstart.md` if anything changed
- [ ] T026 Record CI results for SC-001–SC-006 in `specs/003-model-evals/research.md`

## Dependencies

- Setup → Foundational → US1 → US2, US3 and US4 in any order (they all extend `eval_stack`, so their test edits run in sequence) → US5 → Polish.
- T010 needs T004 and T006; T012 needs T002, T005, T008, T010 and T011; T013 needs T007.

## Parallel examples

- Foundational: T004, T005, T006 and T007 touch different crates or files.
- US1: T010 and T013 in parallel (CLI loop and stub script), then T011, then T012.
- US4: T020 alongside US3's T019.

## Implementation Strategy

MVP = US1: a verdict for a model, against the stub's scripted passing
conversation. Then isolation and repeatability (US2), the hard cases
(US3), history (US4), and CI coverage of the harness (US5).
