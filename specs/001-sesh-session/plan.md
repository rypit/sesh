# Implementation Plan: One-command protected agent session

**Branch**: `001-sesh-session` | **Date**: 2026-11-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/001-sesh-session/spec.md`

## Summary

`sesh` is a Python CLI that glues four existing components together:

- **Splash**: the inference engine, under launchd.
- **SearXNG**: web search, in a host container.
- **sbx**: one microVM sandbox per repository, with deny-by-default
  egress and host-side credential injection.
- **pi**: the harness, inside the sandbox via a custom sbx kit.

sesh compiles a per-sandbox policy from user config, gives the sandbox an
isolated clone of the repository (never the host checkout's `.git`),
returns work as git bundles, and ships its security invariants as
`sesh verify`.

## Technical Context

**Language/Version**: Python 3.12+
**Primary Dependencies**: Typer (CLI), Pydantic v2 (config). Everything
else is an external CLI behind a seam: `sbx`, `launchctl`, `splash`,
`container` (or `docker`), `git`, `security` (Keychain).
**Storage**: config in `~/.config/sesh/config.toml`; state in
`~/.local/state/sesh/` (sessions, isolated clones, bundles, logs). No
database. No secrets in either.
**Testing**: pytest. Two tiers: `unit` (hermetic, seams mocked, runs in
CI) and `invariants` (real sbx and stack on the reference Mac; also
shipped as `sesh verify`).
**Target Platform**: macOS 26.4+ on Apple Silicon M3 or later. Reference
machine: M5 Max, 128 GB.
**Project Type**: single CLI project.
**Performance Goals**: warm `sesh` to the harness prompt in under 10 s;
cold start in under 90 s, excluding downloads (SC-001).
**Constraints**: no sesh daemon; loopback-only host services; minimum
sbx version pinned; no secret values in any file sesh writes.
**Scale/Scope**: one user, one machine, a handful of concurrent sandboxes.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| I. Host is the only trusted zone | Harness, tools and model output run only inside the sandbox. The workspace is an untrusted isolated clone. |
| II. Deny by default | sbx strictest baseline; `policy.py` compiles explicit per-sandbox rules from user config; `sesh policy show` prints them. |
| III. Secrets never enter the sandbox | sbx placeholders (built-in `github`, `set-custom` with `--command` reading the Keychain) and host shims. Never env, files or logs. Invariants INV-S1–S4. |
| IV. Nothing agent-authored runs on host | Isolated clone; host `.git` never mounted; return via in-sandbox `git bundle` and host `git fetch <bundle>`; sesh never runs host git in the clone. Invariant INV-H1. |
| V. Policy comes only from the user | Only `~/.config/sesh/config.toml` feeds the compiler; repo files are never read as config. Invariant INV-P2. |
| VI. Every guarantee is executable | `src/sesh/verify/` holds the invariants, written first (Phase 2 tasks), and run by `sesh verify`. |
| VII. Compose, don't rebuild | Only glue code. One seam per external component. No proxy, sandbox or harness of our own. |
| VIII. One command, sensible defaults | `sesh` does everything with a zero-config default. `sesh init` is idempotent. |

Result: **PASS**, no exceptions. The spec's three clarifications
(FR-011, FR-019, FR-025) were resolved on 2026-11-19 as recommended
below (see spec.md, Clarifications) and don't affect compliance.

### Resolved clarifications (accepted as recommended)

| Item | Recommendation | Why |
|---|---|---|
| FR-011 dirty working tree | Start from `HEAD` plus the uncommitted diff and untracked (non-ignored) files, copied into the isolated clone as one "sesh: carried changes" commit. | Matches what the user sees; nothing is lost; the host tree stays untouched. |
| FR-019 default profile | `online` | The stated workflow needs search and APIs. `offline` is one flag away for sensitive repos. |
| FR-025 sandbox lifetime | Keep per repo; `sesh reset` discards it. | Warm start under 10 s and cached dependencies. The workspace is already isolated from the host. |

## Project Structure

### Documentation (this feature)

```text
specs/001-sesh-session/
├── spec.md          # what and why
├── research.md      # decisions and evidence
├── plan.md          # this file
├── contracts/
│   ├── cli.md       # command surface, exit codes
│   ├── config.md    # config.toml schema
│   └── invariants.md # the verify suite, one row per guarantee
├── quickstart.md    # target user experience, end to end
└── tasks.md         # ordered, test-first work items
```

### Source Code (repository root)

```text
src/sesh/
├── cli.py              # Typer app: parsing, output, exit codes only
├── config.py           # Pydantic models for config.toml (pure, no I/O beyond load)
├── paths.py            # config/state locations
├── doctor.py           # platform and version checks (FR-022, FR-023)
├── init.py             # one-time setup orchestration (US5)
├── stack/
│   ├── launchd.py      # seam: launchctl (plist render + bootstrap/bootout)
│   ├── engine.py       # Splash (oMLX later) behind one interface
│   ├── search.py       # SearXNG via container runtime seam
│   └── health.py       # HTTP health checks
├── sandbox/
│   ├── sbx.py          # seam: the ONLY module that invokes the sbx CLI
│   ├── policy.py       # pure: config -> compiled policy -> sbx commands
│   ├── secrets.py      # credential bindings -> sbx secret commands (no values held)
│   └── kit/            # sbx kit for pi (spec + files/)
├── workspace.py        # isolated clone, carried changes, bundle return (FR-010–012)
├── harness/
│   └── pi/             # models.json template, sesh extensions (search tool)
├── tools.py            # tool declarations -> policy rules + harness wiring (FR-020)
├── shims/              # host-side credential shims (FR-016), one file per API
└── verify/
    ├── runner.py       # runs probes inside a sandbox, reports per invariant
    └── probes/         # the probe scripts copied into the sandbox

tests/
├── unit/               # hermetic; seams mocked
└── invariants/         # pytest wrappers around verify/ for the reference Mac
```

**Structure decision**: one package with three seams that matter
(`sandbox/sbx.py`, `stack/launchd.py`, and the container-runtime call in
`stack/search.py`). `policy.py` is pure, so the most security-relevant
logic is unit-testable without sbx.

## Key flows

**`sesh` in a repository**:

1. `doctor.quick()`: minimum versions present; config valid.
2. `stack.ensure_up()`: launchd-load Splash and SearXNG if not loaded;
   wait for health.
3. `workspace.prepare(repo)`: create or refresh the isolated clone in
   `state/sessions/<repo-id>/`, carrying uncommitted changes (FR-011).
4. `policy.compile(config, profile)` produces the expected rules;
   `sbx.apply(sandbox, rules)`; then `sbx` reads the applied rules back
   and sesh compares them with the expected set (FR-022).
5. `secrets.apply(bindings)`: `sbx secret set github` /
   `set-custom --host … --env … --command "security find-generic-password …"`.
6. `sbx run` the pi kit on the isolated workspace, attached to the
   terminal.

**`sesh pull`**:

1. `sbx exec <sandbox> git bundle create /workspace/.sesh/out.bundle <refs>`.
2. On the host: `git fetch <bundle> 'refs/heads/*:refs/heads/sesh/<session>/*'`
   in the user's checkout. Host git never runs inside the isolated clone.

## Phases

- **Phase 0, research and spikes**: research.md is done; spikes T005–T011
  confirm the behaviors it flags. Each spike ends with a short note in
  research.md and, where behavior differs, a change to this plan.
- **Phase 1, design**: contracts/ and quickstart.md (drafted with this
  plan), revised after the spikes.
- **Phase 2, tasks**: tasks.md, ordered test-first: invariants before
  the code that satisfies them.

## Risks

| Risk | Mitigation |
|---|---|
| sbx custom secrets change format (experimental) | Pin the sbx version; the secret invariants catch regressions on upgrade. |
| #707 any-header substitution | Echo-server invariant; don't bind secrets to echoing hosts; path deny rules. |
| The sbx kit format churns (v2/v3) | Keep the kit tiny; the harness seam allows falling back to sbx's Claude Code kit. |
| Splash's narrow model set | Engine seam with an oMLX implementation planned. |
| Hosted CI can't run sbx (nested virtualization) | Invariants run on the reference Mac before merge; CI runs unit tests only. |
| The user opens the isolated clone in an editor | Documented in the quickstart; the state dir lives under `~/.local/state`, not next to their code. |

## Complexity Tracking

None. No constitution exceptions.
