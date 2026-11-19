# sesh Constitution

sesh ("scribe") runs a coding agent against a local AI stack inside a
sandbox, so that a prompt-injected agent cannot harm the Mac it runs on.
These principles are binding on every spec, plan, task and change. When a
principle and a convenience conflict, the principle wins.

## Core Principles

### I. The host is the only trusted zone

The macOS host (the user's account, Keychain, shell, editor, git config) is
trusted. Everything inside a sandbox is untrusted: the harness, the model's
output, tools running in the sandbox, and **every file the agent writes**.
A repository the user cloned from someone else is untrusted too, before
any agent touches it.

*Rationale*: prompt injection turns any content the agent reads into
instructions. The only boundary we can reason about is the one between
the host and the sandbox.

### II. Deny by default, allow explicitly

Sandboxes start with no network egress and no host filesystem access
beyond their workspace. Every allowance must be:

- declared in user-owned host configuration,
- scoped as narrowly as the runtime allows (host, port, method, path),
- applied per sandbox, and
- inspectable (`sesh policy show` prints exactly what is allowed).

*Rationale*: an allowlist you can read is an allowlist you can audit.

### III. Secrets never enter the sandbox

No real credential may appear in a sandbox's environment, filesystem,
process arguments, or sesh's own logs. Credentials are attached on the
host side: by the sandbox runtime's credential proxy (placeholder
substitution bound to specific hosts) or by a host-side shim service.
The source of truth is the macOS Keychain or a secrets manager, never a
plaintext file in the repo or in sesh's state directory.

*Rationale*: a secret the agent can read is a secret it can leak.

### IV. Nothing agent-authored runs on the host implicitly

sesh must not create a path by which content written in the sandbox
executes on the host without a deliberate, informed user action. Known
vectors include git hooks, repo-level git config (`core.fsmonitor`,
`core.hooksPath`, filters, `diff.external`, pagers), `.envrc`, editor
task and settings files, and build scripts. The agent's work comes back
to the host **as data** (git bundles or patches) for the user to review.

*Rationale*: the VM protects the laptop only until the user runs
something the agent wrote. This is the most likely real-world way to
brick a machine.

### V. Policy comes only from the user

Repository content can never widen what a sandbox may do. There is no
repo-local configuration that grants network access, secrets, mounts or
tools. If repo-local config is ever added, it may only *narrow* policy.

*Rationale*: an attacker controls repo content (Principle I), so repo
content must not control the boundary.

### VI. Every guarantee is executable

Each security guarantee in this constitution has at least one automated
invariant check, run from inside a real sandbox and shipped to users as
`sesh verify`. A change that weakens or removes an invariant must update
this constitution (see Governance) in the same change.

*Rationale*: unverified guarantees decay. Runtime-agnostic invariants
also make it possible to swap the sandbox runtime without losing them.

### VII. Compose, don't rebuild

sesh is glue. It composes existing components (the sandbox runtime, the
inference engine, the harness, the search service) and writes custom
code only where no existing component meets an invariant. Each
component sits behind exactly one module (a seam), so it can be
replaced. Features that are not needed by an accepted spec are not built.

*Rationale*: a small codebase is a small attack surface and a small
maintenance bill. (Lesson from Sovereign: multi-engine routing,
dashboards and benchmark frameworks cost more than they earned for this
use case.)

### VIII. One command, sensible defaults

The happy path is `sesh` in a repo directory, with zero required
configuration after one-time setup. Configuration exists to change
defaults, never to make the default work. Commands are idempotent, and a
warm start (stack up, sandbox exists) is fast.

*Rationale*: if the secure path isn't the easy path, it won't be used.

## Additional Constraints

- **Platform**: macOS on Apple Silicon only (initially M3 or later with
  macOS 26.4+, inherited from the default engine). No Linux or Windows
  support is planned.
- **No sesh daemon**: sesh is a CLI. Long-running processes are the
  components it supervises (engine, search, sandbox runtime), managed
  through launchd or the component's own lifecycle, never by a
  sesh-owned background process.
- **No outbound telemetry** from sesh itself.
- **Pinned minimum versions** for security-relevant components (e.g. the
  sbx version that supports method/path rules). `sesh doctor` refuses to
  run below them.
- **Implementation**: Rust (stable, edition 2024, toolchain pinned),
  one `sesh` binary, `unsafe` code forbidden. Minimal dependencies;
  anything heavier needs justification in a plan (ADR 0002).

## Development Workflow

- Spec-driven: every feature goes through specify → clarify → plan →
  tasks → implement, under `specs/NNN-feature/`.
- Every plan includes a **Constitution Check** that lists each principle
  and says whether the plan complies. Any exception is recorded with a
  justification in the plan's Complexity Tracking section.
- Test-first for invariants: a guarantee's invariant check is written,
  and seen failing, before the code that provides the guarantee.
- Two test tiers:
  - **unit**: hermetic and fast, with external CLIs (sbx, launchctl,
    container runtimes) replaced by fakes at their seam trait. Runs in
    CI.
  - **invariants**: run against a real stack and sandbox on the
    reference Mac before merge. Hosted CI runners likely can't run
    nested VMs; verify this before relying on it.
- Decisions that change a principle, a seam, or the threat model are
  recorded as ADRs in `docs/decisions/`.

## Governance

This constitution supersedes other practices. Amendments require a pull
request that edits this file, explains the change, bumps the version, and
updates any affected invariant checks and templates. Versioning:

- MAJOR: a principle is removed or weakened.
- MINOR: a principle or constraint is added or materially expanded.
- PATCH: clarifications and wording.

All reviews check compliance with this constitution.

**Version**: 1.1.0 | **Ratified**: 2026-11-19 | **Last Amended**: 2026-11-19
