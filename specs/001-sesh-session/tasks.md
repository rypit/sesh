---
description: "Task list for feature 001: sandboxed agent sessions for any terminal harness"
---

# Tasks: Sandboxed agent sessions for any terminal harness

**Input**: Design documents from `specs/001-sesh-session/`: plan.md,
spec.md, research.md, data-model.md, contracts/ (cli, config, recipe,
invariants), quickstart.md

**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Required. The constitution (Development Workflow) mandates
test-first for invariants, and AGENTS.md requires a failing invariant or
unit test before each implementation. Each story phase lists its
invariants first; unit tests are written before the code in the same
task ("Unit tests, then …").

**Organization**: Tasks are grouped by user story so each story can be
implemented and tested on its own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US6)
- Paths follow plan.md "Project Structure" (single Cargo package at the
  repository root)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [x] T001 Run `specify init --here` to add Spec Kit templates, scripts and agent commands to `.specify/` and `.claude/skills/`, keeping `.specify/memory/constitution.md` (done with Spec Kit v1.4.2, Claude integration, `sh` scripts)
- [ ] T002 Create the Rust project skeleton per plan.md: `Cargo.toml` (edition 2024; deps `clap`, `serde`, `toml`, `toml_edit`, `anyhow`, `ureq`, `regex`, `globset`, `sha2`; binaries `sesh` and `sesh-shim`; `invariants` feature), `rust-toolchain.toml`, `src/main.rs`, `src/bin/sesh-shim.rs` (stub that exits 2 with usage), `src/lib.rs` with `#![forbid(unsafe_code)]` and the module tree from plan.md, empty `recipes/`, and a `cargo install --locked --path .` smoke test documented in `README.md`
- [ ] T003 [P] Configure tooling: `Makefile` with `check` = `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`; `deny.toml` (licences, advisories, duplicates); `.github/workflows/ci.yml` running `make check` on a macOS runner without the `invariants` feature
- [x] T004 [P] Add the ADR template `docs/decisions/0000-template.md` (done; ADRs 0001–0005 exist)

---

## Phase 2: Spikes (hands-on, reference Mac; blocks Phase 3)

**Purpose**: Confirm the sbx behaviors research.md flags. Each spike
writes its findings into `specs/001-sesh-session/research.md` (the
section named) and updates plan.md or contracts/ where behavior differs.

- [ ] T005 [P] sbx baseline and policy log (research.md R1, R3): list policy presets and the strictest one's defaults; confirm method/path rules in the installed sbx; set the minimum sbx version for INV-P3; read `content/manuals/ai/governance/local/inspect.md` in docker/docs and find the CLI for denied requests, its output fields, and whether it filters by sandbox and time; record results in `specs/001-sesh-session/research.md`
- [ ] T006 [P] sbx lifecycle for named sandboxes (research.md R4, R7): `sbx create`/`run` with a workspace directory argument, attaching to the terminal, passing args after `--`, exit-code propagation, `sbx exec`, `sbx cp`, removal; record the exact commands the `sandbox/sbx.rs` seam needs in `specs/001-sesh-session/research.md`
- [ ] T007 [P] Host access (research.md R3): from a sandbox, reach a host loopback service via `host.docker.internal:<port>` with a `localhost:<port>` rule; confirm other host ports and LAN addresses are refused; record in `specs/001-sesh-session/research.md`
- [ ] T008 [P] Credentials and logins (research.md R2): `set-custom` with `--command` reading the Keychain; a credential bound to a `localhost` destination; observe the #707 any-header substitution with a local echo server; confirm built-in `github` covers `git push` over HTTPS; confirm Claude Code `/login` (Max, and check Pro) and `sbx secret set openai --oauth` for Codex keep tokens out of the sandbox; check opencode's and pi's login options; record in `specs/001-sesh-session/research.md`
- [ ] T009 [P] Built-in recipes (research.md R4): for `claude`, `codex`, `opencode` and `pi`, confirm the kit or image, config paths to import, credential files, how each learns its model endpoint, and the hosts each needs to work; for pi, build a minimal custom kit or image on sbx's `shell` agent; update the table in `specs/001-sesh-session/research.md` and the examples in `specs/001-sesh-session/contracts/recipe.md`
- [ ] T010 [P] Workspace and snapshot copy (research.md R5, R7): pass an isolated clone directory as the sbx workspace and confirm the host checkout is unreachable; time `sbx cp` of a typical `~/.claude/`; time `git bundle` export and host `git fetch` on a 10,000-file repository against SC-006; record in `specs/001-sesh-session/research.md`

**Checkpoint**: research.md has no open spikes for Phase 3; plan.md and contracts/ reflect any differences.

---

## Phase 3: Foundational (Blocking Prerequisites)

**Purpose**: Config, recipes, policy, seams and the verify runner that every story uses

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T011 [P] Unit tests, then the config models in `src/config.rs` per `specs/001-sesh-session/contracts/config.md` with `#[serde(deny_unknown_fields)]`, implementing data-model.md `Config.default_recipe`, `Config.endpoints`, `Config.credentials`, `Config.deny`, `Config.recipe.<name>` (data-model.md `RecipeOverride.model_endpoint`, `RecipeOverride.extra_endpoints`, `RecipeOverride.env`), `Config.repo.<path>` (data-model.md `RepoScope.extra_endpoints`, `RepoScope.only`), `Config.risky_paths`, `Config.snapshot.max_size`, `Config.snapshot.extra_exclude`; data-model.md `Endpoint.name`, `Endpoint.host`/`Endpoint.local_port`, `Endpoint.scheme`, `Endpoint.methods`/`Endpoint.paths`, `Endpoint.credentials`, `Endpoint.start`, `Endpoint.start_timeout`, `Endpoint.health`, `Endpoint.process`, `Endpoint.required`; data-model.md `CredentialBinding.name`, `CredentialBinding.mode`, `CredentialBinding.service`, `CredentialBinding.source`, `CredentialBinding.hosts`, `CredentialBinding.env`, `CredentialBinding.shim`. Loading rejects literal secret values in any field
- [ ] T012 [P] Unit tests, then recipe loading in `src/recipe.rs` per `specs/001-sesh-session/contracts/recipe.md` and data-model.md Recipe: embedded built-ins from `recipes/` via `include_str!`, user recipes from `~/.config/sesh/recipes/`, user-over-built-in replacement, `RecipeOverride` merge, refusal of any `workspace` other than `repo` or `attach` other than `terminal` (FR-004), reserved subcommand names, unknown substitutions as load errors, and the derived data-model.md `Endpoint.source` (`recipe:<name>`)
- [ ] T013 [P] Unit tests, then the pure policy compiler in `src/sandbox/policy.rs` per the compilation rules in `specs/001-sesh-session/contracts/config.md`: inputs are config, recipe and the repo path used only as a key (FR-013); repo-scope `only` narrows (FR-018); deny wins; credential-host coverage check; deterministic sort and the data-model.md `Sandbox.policy_hash`; no `std::fs`, `std::process` or `std::net`
- [ ] T014 [P] Seam `src/sandbox/sbx.rs`: a trait plus the only implementation invoking the sbx CLI (create, run attached with workspace dir and args, exec, cp, policy allow/deny/read-back, secret set/set-custom, policy log, remove), using the commands recorded by T005–T008; fake implementation in `tests/fakes/sbx.rs`
- [ ] T015 [P] Host seams with traits, macOS implementations and fakes in `tests/fakes/`: `src/host/git.rs` (bundle fetch, status, commit), `src/host/ports.rs` (`lsof` listener and process), `src/host/secret_store.rs` (`security` find/add generic password), `src/host/service_manager.rs` (launchd job install, load, unload, status)
- [ ] T016 [P] Unit tests, then `src/paths.rs` (config and state locations) and `src/lock.rs`: data-model.md `Sandbox.id` (`sesh-<recipe>-<repo-id>`, 12 hex of SHA-256 of the canonical repo path), the per-sandbox lock with live-owner check and stale-lock clearing (FR-026), and the data-model.md Sandbox state file
- [ ] T017 `src/cli.rs` and `src/main.rs`: clap command surface per `specs/001-sesh-session/contracts/cli.md`, recipe-versus-subcommand resolution, passing everything after the recipe name or `--` to the harness unchanged, and the exit-code mapping (0–4)
- [ ] T018 Invariant runner `src/verify/runner.rs`, probe packaging from `src/verify/probes/` (POSIX shell, embedded), the probe server and `std::net` echo server on `127.0.0.1`, a random test credential in its own secret-store item created and removed per run, PASS/FAIL/KNOWN-RISK results per `specs/001-sesh-session/contracts/invariants.md`, and the feature-gated harness `tests/invariants.rs`
- [ ] T019 [P] Unit tests, then quick checks in `src/doctor.rs`: git repo with commits (exit 2), sbx present, logged in and at the minimum version (exit 3), config and recipe validity; each message names the fix (FR-027)

**Checkpoint**: Foundation ready; user story work can begin.

---

## Phase 4: User Story 1 - Run my agent in a sandbox as easily as running it directly (Priority: P1) 🎯 MVP

**Goal**: `sesh <recipe>` opens the user's own harness, with their config and login, in an sbx sandbox that reaches only allowed endpoints.

**Independent Test**: spec.md US1: run `sesh <recipe>` in a sample repo with a harness the user already uses; it opens with their settings, answers through their model endpoint, edits and tests inside the sandbox, and can't read `~/.ssh`.

**Invariants first**: INV-N1, N2, N4, N5, N8, F1, F2, H2, H3, S6, P1, P2, P3, R1, R2.

- [ ] T020 [US1] Write the invariant probes listed above in `src/verify/probes/` and register them in `src/verify/runner.rs`; run them against a bare sandbox and record which fail in `specs/001-sesh-session/contracts/invariants.md`
- [ ] T021 [P] [US1] Unit tests with temporary repos, then `src/workspace.rs`: isolated clone of tracked plus untracked non-ignored files, remote URLs without embedded credentials and no credential-helper config, the carried-changes commit with the `Sesh-Carried-Changes: yes` trailer (FR-011), data-model.md `Sandbox.base` and `Session.base`, and the submodule/LFS warning (FR-027)
- [ ] T022 [P] [US1] Unit tests for each rule, then the pure credential scanner `src/scan.rs` per research.md R6 (token prefixes, PEM headers, secret-named keys with literal values, env-reference exemptions); reports rule names, never values
- [ ] T023 [US1] Unit tests with `tests/fixtures/home/`, then snapshot staging in `src/snapshot.rs` per research.md R5: recipe `import.paths`, `credential_files` and `exclude`, data-model.md `Config.snapshot.extra_exclude`, symlink-escape skipping, the data-model.md `Config.snapshot.max_size` cap, setuid/setgid stripping, scanning with `src/scan.rs`, and the data-model.md ConfigSnapshot exclusion list with reasons
- [ ] T024 [P] [US1] Unit tests with fakes, then `src/endpoint.rs` per research.md R8: reachability for data-model.md `Endpoint.required` endpoints, running data-model.md `Endpoint.start` and killing it after `Endpoint.start_timeout`, `Endpoint.health` checks, and loopback-only plus data-model.md `Endpoint.process` listener checks via `src/host/ports.rs` (exit 3 on failure, FR-027)
- [ ] T025 [P] [US1] Unit tests with fakes, then `src/sandbox/secrets.rs` for `builtin` and `header` modes: data-model.md `CredentialBinding.service` via `sbx secret set <service>`, and data-model.md `CredentialBinding.source` (`keychain:`, `1password:`, `command:`) with `CredentialBinding.hosts` and `CredentialBinding.env` via `sbx secret set-custom`; no value held longer than the sbx call (FR-019)
- [ ] T026 [P] [US1] Built-in recipes `recipes/claude.toml`, `recipes/codex.toml` and `recipes/opencode.toml` using sbx kits and built-in secret services, per T009 findings and `specs/001-sesh-session/contracts/recipe.md`; a unit test in `src/recipe.rs` loads and validates every built-in recipe
- [ ] T027 [P] [US1] Built-in recipe `recipes/pi.toml` and its custom kit or image definition in `recipes/pi/`, built on sbx's `shell` agent per T009, with the generated `models.json` file entry
- [ ] T028 [US1] Session orchestration in `src/session.rs` per plan.md "Key flows" steps 1–9: resolve recipe, quick doctor, lock, endpoints, workspace, compile and apply policy with read-back (exit 4 on mismatch), secrets, snapshot copy with `sbx cp` and recipe `files`, then run the harness attached with recipe `env` and user args; release the lock and map exit codes on every path, including an abnormal harness exit (FR-027)
- [ ] T029 [US1] Warm-start path in `src/session.rs`: reuse the sandbox and skip unchanged steps (policy hash equal, clone current); measure both halves of SC-001 on the reference Mac and record them in `specs/001-sesh-session/research.md`
- [ ] T030 [US1] Run the US1 invariants until all PASS, and check SC-002 with each built-in recipe against the user's real harness config (zero edits); record results in `specs/001-sesh-session/contracts/invariants.md`

**Checkpoint**: `sesh <recipe>` opens the user's harness in a sandbox with their config; work persists between runs.

---

## Phase 5: User Story 2 - Get the agent's work back without ceremony, and safely (Priority: P1)

**Goal**: when the harness exits, the agent's commits are on `sesh/<session>/<branch>` in the host checkout, risky files are flagged, and nothing planted runs on the host.

**Independent Test**: spec.md US2: plant a git hook, `core.fsmonitor`, `.envrc`, a Makefile target and a CI file, commit, exit; the work returns as a branch, the risky files are listed, and ordinary host git runs none of the planted content.

**Invariants first**: INV-H1.

- [ ] T031 [US2] Write the INV-H1 planted-content probe (git hook, `core.fsmonitor`, `.envrc`, Makefile target, `.claude/settings.json` hook, host sentinel file) in `src/verify/probes/` and register it in `src/verify/runner.rs`; run it and record that it fails before T034
- [ ] T032 [P] [US2] Unit tests with fakes, then `src/retrieve.rs`: in-sandbox `git bundle create` from data-model.md `Session.base` for every branch, host fetch into `sesh/<session>/<branch>` via `src/host/git.rs`, data-model.md `Session.returned` recorded in the state dir, the carried-changes commit listed separately, and uncommitted sandbox work reported with an offer to commit it inside the sandbox first (FR-015)
- [ ] T033 [P] [US2] Unit tests, then the risky-path matcher `src/risky.rs`: the built-in list from research.md R7 plus data-model.md `Config.risky_paths`, matched against files changed in returned commits
- [ ] T034 [US2] Wire return into `src/session.rs` (plan.md "Key flows" step 10, on every exit path) and the `sesh pull` command in `src/cli.rs`; print the data-model.md ReturnSummary per `specs/001-sesh-session/contracts/cli.md` output rules
- [ ] T035 [US2] Session ids in `src/session.rs`: data-model.md `Session.id` (`YYYYMMDD-<n>`), continuation across runs, and `sesh --new`
- [ ] T036 [US2] Run INV-H1 until PASS; record SC-005, and SC-006 (5 s for 10,000 files) in `specs/001-sesh-session/research.md`

**Checkpoint**: US1 and US2 together are the usable MVP: work goes in and comes back safely.

---

## Phase 6: User Story 3 - Give the agent exactly the access it needs, without exposing secrets (Priority: P1)

**Goal**: endpoints and credentials from user config only, refused hosts reported, `sesh allow` to grant, shims for credentials sbx can't inject.

**Independent Test**: spec.md US3: call the model provider, install a package and call a configured API; no real credential is visible inside; a request to an unlisted host fails, is listed after exit, and succeeds after `sesh allow`.

**Invariants first**: INV-N6, N7, N9, S1, S2, S3, S4, S5.

- [ ] T037 [US3] Write the network and secrets probes listed above in `src/verify/probes/`, using the echo server and test credential from T018, and register them in `src/verify/runner.rs`; run and record which fail
- [ ] T038 [P] [US3] Unit tests with fakes, then the refused-host report in `src/report.rs`: read the sbx policy log for the sandbox and session window (format from T005), keep hosts and counts only, and print a `sesh allow` line per host; call it from `src/session.rs` after return (FR-021)
- [ ] T039 [P] [US3] Unit tests, then `sesh allow` in `src/allow.rs` per `specs/001-sesh-session/contracts/config.md` "sesh allow writes": append to data-model.md `RepoScope.extra_endpoints`, or to data-model.md `Config.endpoints` with `--global`, using `toml_edit` to keep comments; print the added TOML; never add credentials
- [ ] T040 [US3] `sesh policy show` in `src/cli.rs`: the compiled policy with each endpoint's data-model.md `Endpoint.source`, method/path and deny rules, and credential bindings by name and host only, with `--json` (FR-025)
- [ ] T041 [US3] Shim mode: `src/bin/sesh-shim.rs`, the shim framework in `src/shims/mod.rs`, and a reference HTTP Basic shim in `src/shims/basic_auth.rs` bound to `127.0.0.1` that reads its secret per request and forwards only to its upstream; install its job via `src/host/service_manager.rs` from data-model.md `CredentialBinding.shim`; justify its HTTP server and TLS client crates in `specs/001-sesh-session/plan.md` (ADR 0003)
- [ ] T042 [US3] Run the US3 invariants: S3 reports KNOWN-RISK (#707), everything else PASS; record SC-004 and SC-007 in `specs/001-sesh-session/contracts/invariants.md`

**Checkpoint**: deny-by-default with a one-command way to grant access; secrets never inside.

---

## Phase 7: User Story 4 - Verify my protections (Priority: P2)

**Goal**: `sesh verify` reports every invariant, independent of harness, and pre-launch checks block a bad session.

**Independent Test**: spec.md US4: all PASS on a correct setup; allowing `example.com` makes INV-N1 FAIL with expected vs observed.

**Invariants first**: INV-P4.

- [ ] T043 [US4] Write the INV-P4 probe (no resident `sesh` process; every `sesh-shim` process is a service-manager job) in `src/verify/probes/` and register it
- [ ] T044 [US4] `sesh verify` in `src/cli.rs` using `src/verify/runner.rs`: table output, `--only`, `--json`, exit codes per `specs/001-sesh-session/contracts/cli.md`
- [ ] T045 [US4] Pre-launch checks (FR-023) in `src/session.rs`: policy read-back versus data-model.md `Sandbox.policy_hash` (exit 4), minimum versions and required endpoints; tests for each refusal
- [ ] T046 [US4] For each invariant, remove its protection and confirm it FAILs (SC-003); keep the list in `specs/001-sesh-session/contracts/invariants.md`

**Checkpoint**: every guarantee has a failing-when-broken check.

---

## Phase 8: User Story 5 - Bring a harness sesh doesn't know yet (Priority: P2)

**Goal**: a user-written recipe makes any terminal harness work like a built-in one.

**Independent Test**: spec.md US5: write a recipe for a harness without a built-in one, using only the documented format, and run a session that meets US1's scenarios.

- [ ] T047 [P] [US5] `sesh recipes` and `sesh recipes show <name>` in `src/cli.rs`: list recipes with source (built-in, user, override) and print one merged recipe
- [ ] T048 [P] [US5] Unit tests, then the repository-recipe notice in `src/recipe.rs`: detect `.sesh/recipes/` and harness-like config in the repository, ignore it, and say so (US5-3, Principle V); invalid user recipes refused with the problem named (US5-4)
- [ ] T049 [P] [US5] Recipe-writing guide `docs/recipes.md` derived from `specs/001-sesh-session/contracts/recipe.md`, with an example recipe for a harness that has no built-in one in `docs/examples/recipes/`
- [ ] T050 [US5] On the reference Mac, run the example recipe from `docs/examples/recipes/` through US1's acceptance scenarios and record the result in `specs/001-sesh-session/quickstart.md`

**Checkpoint**: "any harness" is demonstrated with a recipe sesh doesn't ship.

---

## Phase 9: User Story 6 - Easy installation (Priority: P3)

**Goal**: install plus `sesh init` gets a first session working in at most three commands, without editing files.

**Independent Test**: spec.md US6: on a fresh macOS account, follow quickstart.md; the first session works with no file edits.

- [ ] T051 [US6] Unit tests with fakes, then `sesh init` in `src/init.rs`: platform gate (macOS 14+, Apple Silicon) before any change; install and log in to sbx; choose data-model.md `Config.default_recipe` and the model endpoint interactively; store credentials with `sbx secret set <service>` or `src/host/secret_store.rs`; write the default config with registry endpoints via `toml_edit`; idempotent (FR-006, FR-024)
- [ ] T052 [P] [US6] Full `sesh doctor` and `--probe` in `src/doctor.rs`: endpoints reachable and loopback-only, shim jobs, and one test request with a tool definition to the default recipe's model endpoint (research.md R8)
- [ ] T053 [P] [US6] `sesh ls`, `sesh reset` and `sesh logs <shim>` in `src/cli.rs`: `reset` lists commits not in data-model.md `Session.returned`, confirms, then removes the sandbox, clone and state (FR-025)
- [ ] T054 [US6] Fresh-account walkthrough of `specs/001-sesh-session/quickstart.md` on the reference Mac; record SC-008

**Checkpoint**: a new user goes from install to first session without editing a file.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T055 [P] Bring `README.md` and `specs/001-sesh-session/quickstart.md` in line with actual behavior, and write the threat model in `docs/threat-model.md`
- [ ] T056 [P] Architecture overview in `docs/architecture.md`, plus ADRs in `docs/decisions/` for anything the spikes changed
- [ ] T057 Run the full `sesh verify` on the reference Mac, confirm SC-001 through SC-009, and tag v0.1.0

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Spikes (Phase 2)** → **Foundational (Phase 3)** → user stories.
- T002 must land before T003. Spikes need only an installed sbx and can run while T002/T003 are in review.
- **US1 (Phase 4)** blocks every other story: it builds the session flow they extend.
- **US2 (Phase 5)** starts after T021 and T028.
- **US3 (Phase 6)** and **US4 (Phase 7)** can run in parallel after US1.
- **US5 (Phase 8)** needs US1; **US6 (Phase 9)** needs US1 and US3 (credentials and `toml_edit` config writing).
- **Polish (Phase 10)** last.

### Within Each Story

- Invariant probes first, seen failing → unit tests → implementation → invariant run.
- `src/session.rs` tasks are sequential (T028 → T029 → T034 → T035 → T045).

### Parallel Opportunities

- Spikes T005–T010 are all independent.
- Foundational: T011–T016 and T019 touch different files; T017 waits for T011 and T012; T018 waits for T014.
- US1: T021, T022, T024, T025, T026, T027 in parallel; T023 after T022; T028 after all of them.
- US2: T032 and T033 in parallel.
- US3: T038 and T039 in parallel.
- US5: T047, T048, T049 in parallel.
- US6: T052 and T053 in parallel.

## Parallel Example: User Story 1

```text
# After T020 records the failing invariants, launch together:
T021 workspace (src/workspace.rs)
T022 credential scanner (src/scan.rs)
T024 endpoints (src/endpoint.rs)
T025 secrets (src/sandbox/secrets.rs)
T026 claude/codex/opencode recipes (recipes/*.toml)
T027 pi recipe and image (recipes/pi.toml, recipes/pi/)
# Then T023 (needs scan.rs), then T028 (needs all).
```

## Implementation Strategy

### MVP First (US1 + US2)

1. Phase 1 Setup, Phase 2 Spikes, Phase 3 Foundational.
2. Phase 4 US1: a sandboxed session with the user's config.
3. Phase 5 US2: work comes back safely. **Stop and validate**: US1 alone
   strands the agent's work in the sandbox, so the first usable release
   is US1 + US2.

### Incremental Delivery

1. MVP (US1 + US2) → validate with quickstart.md steps 2–3.
2. US3 → refused-host report, `sesh allow`, shims.
3. US4 → `sesh verify` complete, each invariant seen failing.
4. US5 → user recipes documented and demonstrated.
5. US6 → `sesh init` and the fresh-account walkthrough.
6. Polish → v0.1.0.

## Notes

- [P] = different files, no dependency on an incomplete task.
- Never write secret values to any file, log, fixture or commit (AGENTS.md).
- Only `src/sandbox/sbx.rs` invokes `sbx`; `src/sandbox/policy.rs` and `src/scan.rs` stay pure.
- Spike numbering (T005–T010) matches the references in research.md.
