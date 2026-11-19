# Tasks: One-command protected agent session

**Input**: [plan.md](./plan.md), [spec.md](./spec.md),
[research.md](./research.md), [contracts/](./contracts/)

**Format**: `T### [P?] [Story?] Description`
- **[P]**: can run in parallel (different files, no dependency).
- **[USn]**: user story from spec.md.
- Test-first (constitution, Development Workflow): each invariant task
  comes before the implementation that satisfies it, and must be seen
  failing first.

---

## Phase 1: Setup

- [x] T001 Run `specify init --here` to add Spec Kit templates, scripts
  and agent commands. Confirm it keeps
  `.specify/memory/constitution.md`. (Done with Spec Kit v1.4.2,
  `--integration claude --script sh`. The constitution was kept. Add
  pi later with Spec Kit's integration support if wanted.)
- [ ] T002 [P] Python project skeleton: `pyproject.toml` (Python 3.12,
  Typer, Pydantic), `src/sesh/__init__.py`, the `sesh` entry point, a
  `uv tool install .` smoke test.
- [ ] T003 [P] Tooling: ruff, mypy (strict on `src/sesh`), pytest with
  `unit` and `invariants` markers, `make check`, CI running unit tests
  only.
- [ ] T004 [P] `docs/decisions/` with ADR template; ADR 0001 records R1
  (sbx) and R6 (isolated workspace) as the founding decisions.

## Phase 2: Spikes (hands-on, reference Mac)

Each spike writes its findings back into research.md and updates
plan.md or the contracts where behavior differs.

- [ ] T005 [P] sbx baseline: list the policy presets and the strictest
  one's defaults; confirm method/path rules exist in the installed
  version; set the minimum sbx version pin.
- [ ] T006 [P] Splash: default port, loopback bind flag, `SPLASH_API_KEY`
  behavior, running under a launchd user agent, health endpoint, and
  model download location and size.
- [ ] T007 [P] Host access: from a sandbox, reach a host loopback service
  via `host.docker.internal:<port>` with a `localhost:<port>` rule;
  confirm other host ports and LAN addresses are refused.
- [ ] T008 [P] Secrets: `set-custom` with `--command` reading the
  Keychain; behavior with a `localhost` destination; observe the #707
  any-header substitution with a local echo server; confirm the built-in
  `github` service covers `git push` over HTTPS.
- [ ] T009 [P] Kit: build a minimal sbx kit that installs pi and launches
  it; determine the current kit format (v1 spec.yaml / v2 / v3); confirm
  `models.json` pointing at `host.docker.internal` works with Splash
  (`openai-completions` vs `anthropic-messages`).
- [ ] T010 [P] Workspace: how `sbx run` exposes a host directory (bind
  mount vs sync; read-only or exclusion options); confirm the host
  checkout's `.git` stays unreachable when the workspace is an isolated
  clone; test `git bundle` export and import.
- [ ] T011 [P] Search: run SearXNG with Apple `container`, published on
  `127.0.0.1` only, JSON format enabled, under launchd; fall back to
  Docker/OrbStack if needed.
- [ ] T012 Resolve the spec's three NEEDS CLARIFICATION items (FR-011,
  FR-019, FR-025) with the user, using plan.md's recommendations. Update
  spec.md.

## Phase 3: Foundational (blocks all user stories)

- [ ] T013 [P] Unit tests, then `src/sesh/config.py`: config.toml models
  per contracts/config.md (all optional, `extra="forbid"`, no secret
  values accepted).
- [ ] T014 [P] Unit tests, then `src/sesh/sandbox/policy.py`: pure
  compiler config → sorted rule list (both profiles; deny wins;
  deterministic).
- [ ] T015 [P] Seam `src/sesh/sandbox/sbx.py`: the only module invoking
  the sbx CLI (run, exec, policy apply/read-back, secret set/set-custom,
  reset). Unit tests mock it here.
- [ ] T016 [P] Seam `src/sesh/stack/launchd.py`: render plists,
  bootstrap/bootout, status.
- [ ] T017 Invariant runner `src/sesh/verify/runner.py` + probe
  packaging: copy probes into a sandbox, run them, collect
  PASS/FAIL/KNOWN-RISK per contracts/invariants.md.
- [ ] T018 [P] `src/sesh/doctor.py`: platform checks (chip, macOS,
  memory), minimum versions, config validity.

## Phase 4: US1, protected session (P1) 🎯 MVP

**Invariants first**: INV-N1, N2, N4, N5, F1, F2, P1, P3, R1, R2.

- [ ] T019 [US1] Write invariant probes listed above; run against a bare
  sandbox and record which fail.
- [ ] T020 [P] [US1] `stack/engine.py` + `stack/health.py`: Splash under
  launchd, loopback only, health wait.
- [ ] T021 [P] [US1] `stack/search.py`: SearXNG per T011.
- [ ] T022 [US1] `workspace.py`: isolated clone in the state dir; carried
  changes per the FR-011 decision.
- [ ] T023 [US1] `sandbox/kit/` + `harness/pi/`: the pi kit with
  `models.json` for the local engine.
- [ ] T024 [US1] `sesh` command flow (plan.md "Key flows"): quick doctor
  → stack up → workspace → policy apply + read-back → secrets → run kit.
- [ ] T025 [US1] Warm-start path: reuse the stack and sandbox; measure
  against SC-001.
- [ ] T026 [US1] Run the US1 invariants: all PASS.

**Checkpoint**: `sesh` opens pi in a sandbox against Splash; work
persists between sessions.

## Phase 5: US2, safe return (P1)

**Invariants first**: INV-H1, H2.

- [ ] T027 [US2] Write the INV-H1 planted-content probe (hook,
  fsmonitor, `.envrc`, Makefile, with a host sentinel file) and INV-H2.
- [ ] T028 [US2] `sesh pull`: in-sandbox `git bundle create`, host
  `git fetch <bundle>` into `sesh/<session>/*`; report uncommitted
  sandbox work.
- [ ] T029 [US2] Run INV-H1/H2: PASS; record SC-005.

## Phase 6: US3, web and secrets (P2)

**Invariants first**: INV-N3, N6, N7, S1–S5.

- [ ] T030 [US3] Echo server + test credential fixtures for
  `sesh verify` (created and removed per run).
- [ ] T031 [US3] Write the network and secrets probes listed above.
- [ ] T032 [P] [US3] `sandbox/secrets.py`: builtin and header bindings
  from config → sbx commands; Keychain/1Password/command sources; no
  values held in Sesh's process longer than the sbx call.
- [ ] T033 [P] [US3] pi search extension calling SearXNG JSON, declared
  as a built-in tool via `tools.py`.
- [ ] T034 [US3] Online/offline profiles wired through the compiler and
  the `--profile` flag.
- [ ] T035 [US3] Run the US3 invariants; S3 reports KNOWN-RISK (#707);
  everything else PASS. Record SC-003 and SC-006.

## Phase 7: US4, verify (P2)

- [ ] T036 [US4] `sesh verify` command: table output, `--json`, exit
  codes per contracts/cli.md.
- [ ] T037 [US4] Pre-launch fast checks (FR-022) wired into `sesh`; exit
  4 on mismatch.
- [ ] T038 [US4] For each invariant, remove its protection and confirm
  it FAILs (SC-002); keep the list in contracts/invariants.md.

## Phase 8: US5, install (P3)

- [ ] T039 [US5] `sesh init`: platform gate before any change; install
  Splash, the model, SearXNG, sbx and the kit; write the default config;
  idempotent.
- [ ] T040 [US5] `sesh policy show`, `ls`, `reset`, `logs`,
  `stack up|down|status`.
- [ ] T041 [US5] Fresh-account walkthrough of quickstart.md; record
  SC-004.

## Phase 9: US6, custom tools (P3)

**Invariants first**: INV-P2, plus a tool-scoped variant of S1.

- [ ] T042 [US6] Tool declarations in config → policy rules + harness
  wiring; repo-declared tools ignored, with a message.
- [ ] T043 [US6] First shim in `shims/` (an API that needs query-param or
  Basic auth) as the reference custom tool.
- [ ] T044 [US6] Run INV-P2 and the tool secret check.

## Phase 10: Polish

- [ ] T045 [P] README, quickstart and threat-model doc in sync with
  behavior.
- [ ] T046 [P] Architecture doc + ADRs for anything the spikes changed.
- [ ] T047 Full `sesh verify` on the reference Mac; tag v0.1.0.

## Dependencies

- Phase 1 → Phase 2 → Phase 3 → user-story phases.
- US1 (Phase 4) blocks everything else. US2 can start once T022 lands.
  US3 and US4 can run in parallel after US1. US5 and US6 come after US3.
- Within a phase: invariant probes → implementation → invariant run.
