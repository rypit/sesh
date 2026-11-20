# Implementation Plan: Sandboxed agent sessions for any terminal harness

**Branch**: `001-sesh-session` | **Date**: 2026-11-20 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-sesh-session/spec.md`
(rewritten 2026-11-19, ADR 0004)

## Summary

`sesh <recipe>` runs the user's own terminal harness in an sbx microVM,
as easily as running the harness directly. sesh:

- reads a data-only **recipe** for the harness (install, launch, config
  to import, credential files, endpoints, endpoint wiring);
- copies the user's harness config in as a **snapshot**, with
  credentials and secret-looking files removed, never synced back;
- compiles a deny-by-default **policy** from user config and the recipe
  only, attaching credentials through sbx injection or `sesh-shim`;
- gives the sandbox an **isolated clone** of the repository and, when
  the harness exits, **returns** the agent's commits as a git bundle
  onto `sesh/<session>/<branch>`, flagging risky paths;
- reports **refused hosts** and lets `sesh allow` grant them;
- ships its guarantees as **`sesh verify`**.

sesh does not run inference or search; those are endpoints the user
declares (research.md R8).

## Technical Context

**Language/Version**: Rust, stable, edition 2024, pinned in
`rust-toolchain.toml`; target `aarch64-apple-darwin` (ADR 0002).

**Primary Dependencies**: `clap` (CLI), `serde` + `toml` (config and
recipes), `anyhow` (errors), `ureq` (endpoint health checks), `regex`
(snapshot credential scan, research.md R6), `globset` (recipe
exclusions, risky-path list), `sha2` (repo ids, policy hash),
`toml_edit` (`sesh allow` and `sesh init` edit the user's config without
losing comments). Shim
HTTP/TLS crates belong to `sesh-shim` only and are justified when the
first shim lands (ADR 0003). External CLIs, each behind one seam:
`sbx`, `git`, `lsof`, and on macOS `launchctl` and `security`
(Keychain), the last two behind OS-neutral seams (ADR 0005).

**Storage**: user config in `~/.config/sesh/config.toml` and user recipes
in `~/.config/sesh/recipes/`; state in `~/.local/state/sesh/`
(`sandboxes/<id>/` with state, lock, isolated clone, returned-commit
record; `snapshots/<id>/` staging; `logs/`). No database. No secrets in
any of it.

**Testing**: `cargo test`. Two tiers: `unit` (hermetic; every seam has
a fake through its trait; runs in CI on a macOS runner) and `invariants`
(behind the `invariants` Cargo feature; real sbx on the reference Mac;
also shipped as `sesh verify`). Recipe files are unit-tested by loading
and validating every built-in recipe.

**Target Platform**: macOS 14+ on Apple Silicon (sbx's minimum).
Linux later: only `host/secret_store.rs` and `host/service_manager.rs`
need second implementations (ADR 0005).

**Project Type**: single Cargo package, two binaries: the `sesh` CLI and
`sesh-shim` (credential shims, run only by launchd).

**Performance Goals**: warm `sesh <recipe>` to the harness prompt in
under 10 s; first session in a new repository under 90 s excluding
downloads (SC-001); returned work available within 5 s of harness exit
for a 10,000-file repository (SC-006).

**Constraints**: no sesh daemon (only launchd-run `sesh-shim` processes
remain, INV-P4); local endpoints loopback-only (INV-N8); minimum sbx
version pinned (INV-P3); no secret values in any file sesh writes
(INV-S5); policy from user config and recipes only (INV-P2).

**Scale/Scope**: one user, one machine, a handful of sandboxes; four
built-in recipes.

**Open questions** (no NEEDS CLARIFICATION remain; these are spikes on
the reference Mac, research.md): sbx baseline preset and policy-log
format (R3), local-endpoint rule syntax (R3), subscription logins
(R2), each recipe's paths and wiring (R4), snapshot copy mechanism and
workspace exposure (R5, R7).

## Constitution Check

*GATE: checked before Phase 0 and again after Phase 1 design. Both pass.*

| Principle / constraint | How this plan complies |
|---|---|
| I. Host is the only trusted zone | Harness, imported config, tools and model output run only inside the sandbox. The workspace is an untrusted isolated clone; the snapshot is a copy, not a mount of host originals. |
| II. Deny by default | sbx strictest baseline; `policy.rs` compiles explicit per-sandbox rules from config and recipe; `sesh policy show` prints each rule with its source. Refused hosts are reported, never auto-allowed. |
| III. Secrets never enter the sandbox | sbx placeholders or `sesh-shim`; recipe credential files excluded and the snapshot scanned (R5, R6); logins that can't be attached host-side are refused, not copied in (FR-020). INV-S1–S6. |
| IV. Nothing agent-authored runs on host | Isolated clone, host `.git` never mounted, return as an in-sandbox bundle fetched on the host; config snapshot never synced back; risky paths flagged. INV-H1–H3. |
| V. Policy comes only from the user | Inputs are user config plus built-in or user recipes; the repo path is only a key into `repo.<path>`. Repo-local recipes and config ignored and reported. INV-P2. |
| VI. Every guarantee is executable | Each guarantee above maps to an invariant in contracts/invariants.md, written before its code (tasks.md). |
| VII. Compose, don't rebuild | sbx for isolation, injection and egress; user's harness via recipes; user's endpoints. sesh writes only policy compilation, snapshot, return, reporting and verify. No inference, search or harness code. |
| VIII. One command, sensible defaults | `sesh` / `sesh <recipe>` with config written by `sesh init`; built-in recipes need no edits (SC-002). Idempotent commands; warm start reuses the sandbox. |
| No sesh daemon | `sesh` exits with the harness; start commands must return (R8); shims are launchd jobs (ADR 0003). INV-P4. |
| No outbound telemetry | sesh sends none; built-in recipes leave harness telemetry hosts blocked (R4). |
| Pinned minimum versions | sbx only (INV-P3); harness versions are the recipe's concern. |
| Implementation | Rust, `unsafe` forbidden; new crates (`regex`, `globset`, `sha2`) justified in Technical Context and research.md. |

Result: **PASS**, no exceptions.

## Project Structure

### Documentation (this feature)

```text
specs/001-sesh-session/
├── spec.md          # what and why (rewritten, ADR 0004)
├── research.md      # decisions, spikes, prior art
├── plan.md          # this file
├── data-model.md    # entities, fields, state
├── contracts/
│   ├── cli.md       # command surface, exit codes
│   ├── config.md    # config.toml schema, compilation rules
│   ├── recipe.md    # recipe format, built-in examples
│   └── invariants.md # the verify suite
├── quickstart.md    # validation walkthroughs for US1–US6
├── checklists/requirements.md
└── tasks.md         # regenerated by /speckit-tasks
```

### Source Code (repository root)

```text
Cargo.toml            # one package; bins `sesh`, `sesh-shim`; `invariants` feature
rust-toolchain.toml
deny.toml             # cargo-deny: licences, advisories, duplicates
recipes/              # built-in recipes (claude, codex, opencode, pi), embedded with include_str!
src/
├── main.rs           # `sesh`: thin, calls cli::run, maps errors to exit codes
├── bin/
│   └── sesh-shim.rs  # `sesh-shim <name>`: one shim; started only by launchd
├── lib.rs            # #![forbid(unsafe_code)]; module tree
├── cli.rs            # clap: parsing, recipe-vs-subcommand resolution, output, exit codes
├── config.rs         # serde models for config.toml (deny_unknown_fields; load only)
├── paths.rs          # config/state locations
├── recipe.rs         # recipe models, loading (built-in + user), override merge, validation
├── endpoint.rs       # endpoint checks: reachability, loopback, start command, health
├── snapshot.rs       # stage the config snapshot: exclusions, symlink rule, size cap
├── scan.rs           # pure: credential rule set over file contents
├── workspace.rs      # isolated clone, carried-changes commit, session base
├── session.rs        # orchestrates the `sesh [<recipe>]` flow (Key flows steps 1–10)
├── retrieve.rs       # bundle return, returned-commit record, summary
├── risky.rs          # risky-path matcher (built-in list + config)
├── allow.rs          # `sesh allow`: edit user config, print the change
├── report.rs         # refused-host report from the policy log
├── doctor.rs         # platform, versions, config, endpoints, probe
├── init.rs           # one-time setup
├── lock.rs           # per-sandbox lock with stale detection (FR-026)
├── sandbox/
│   ├── sbx.rs        # seam: the ONLY module that invokes the sbx CLI
│   ├── policy.rs     # pure: config + recipe -> compiled policy -> sbx commands
│   └── secrets.rs    # credential bindings -> sbx secret commands / shim jobs
├── host/
│   ├── git.rs        # seam: host git (fetch bundle, status, commit for carried changes)
│   ├── service_manager.rs # seam: shim jobs (macOS impl: launchctl)
│   ├── ports.rs      # seam: lsof listener checks
│   └── secret_store.rs # seam: credential sources (macOS impl: `security`)
├── shims/            # credential shims (FR-020), one module per API; used only by sesh-shim
└── verify/
    ├── runner.rs     # runs probes in a sandbox and on the host, reports per invariant
    └── probes/       # POSIX shell probes, embedded and copied into the sandbox

tests/
├── fakes/            # fake seam implementations shared by tests
├── fixtures/home/    # fake harness config homes for snapshot and scan tests
└── invariants.rs     # #[cfg(feature = "invariants")]: runs verify/ on the reference Mac
```

(Unit tests live next to the code in `#[cfg(test)]` modules.)

**Structure decision**: one package, five seams (`sandbox/sbx.rs`,
`host/git.rs`, `host/service_manager.rs`, `host/ports.rs`,
`host/secret_store.rs`),
each a trait with one real implementation over `std::process::Command`
and a fake for tests. `policy.rs` and `scan.rs` are pure, so the most
security-relevant logic is unit-testable without sbx. Harness knowledge
is only in `recipes/` (FR-002).

## Key flows

**`sesh [<recipe>] [args…]` in a repository**:

1. Resolve the recipe (built-in, user, override) and validate it.
2. `doctor::quick()`: git repo with commits; sbx present, logged in, at
   the minimum version; config valid.
3. `lock::acquire(sandbox)`: refuse (exit 1) if held by a live process;
   clear a stale lock.
4. `endpoint::ensure(required)`: reachable, else run `start` and wait;
   local ones loopback-only and held by the expected process (exit 3
   otherwise).
5. `workspace::prepare()`: create or refresh the isolated clone (never
   sbx `--clone`, ADR 0005), with remote URLs stripped of embedded
   credentials and no credential-helper config; record the base;
   carried-changes commit with trailer; warn on submodules and LFS.
6. `policy::compile(config, recipe, repo_key)`; `sbx` applies it; read
   back and compare with the compiled hash (exit 4 on mismatch).
7. `secrets::apply(bindings)`: sbx secrets; ensure shim jobs are loaded.
8. `snapshot::stage(recipe)` then copy into the sandbox home; write the
   recipe's `files`; report exclusions.
9. `sbx` runs the recipe command attached to the terminal, with `env`
   and the user's args.
10. On exit, whatever the exit code: `retrieve::run()` (bundle,
    fetch, record, summary), `report::refused()`, release the lock, exit
    with the mapped code.

**`sesh pull`**: steps 1–3 and 10 without running the harness.

**`sesh allow <host>`**: load config, append one endpoint to the repo
scope or global list, write the file, print the added TOML. No sandbox
changes until the next session (step 6).

## Phases

- **Phase 0, research**: research.md updated for the rewrite; spikes
  (tasks.md Phase 2) confirm the sbx behaviors it flags and each
  built-in recipe.
- **Phase 1, design**: data-model.md, contracts/ (cli, config, recipe,
  invariants) and quickstart.md, revised after the spikes.
- **Phase 2, tasks**: tasks.md regenerated by `/speckit-tasks`, ordered
  test-first: invariants before the code that satisfies them.

## Risks

| Risk | Mitigation |
|---|---|
| Subscription logins can't be attached host-side | Spike first; API keys are the supported path; sesh refuses rather than copying tokens in (FR-020). |
| A harness moves its config or credential files | Recipes are data, validated in CI; INV-S6 runs against each installed recipe's import rules; credential scan as backstop. |
| A credential in an unknown format slips past the scan | Recipe `credential_files` is the primary control; the scan is a backstop; snapshot exclusions are user-extendable. |
| Harnesses misbehave when telemetry/update hosts are blocked | Refused-host report plus `sesh allow`; recipes may declare hosts genuinely needed to work. |
| sbx custom secrets change format (experimental) | Pin sbx; secret invariants catch regressions. |
| #707 any-header substitution | Echo-server invariant (KNOWN-RISK); don't bind secrets to echoing hosts; path deny rules. |
| The sbx kit format churns | Recipes may use an OCI image instead of a kit. |
| Policy log lacks non-HTTP denials | Report says so; INV-N4 still proves they're blocked. |
| Hosted CI can't run sbx | Invariants run on the reference Mac before merge; CI runs unit tests only. |
| The user opens the isolated clone in an editor | Documented; the state dir lives under `~/.local/state`. |

## Complexity Tracking

None. No constitution exceptions.
