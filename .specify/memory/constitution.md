# sesh Constitution

sesh ("scribe") is a local-first agent platform for one household: AI
agents act on each user's behalf through a single trusted gateway, so no
agent ever holds a secret. It runs in two modes on one stack:

- **Assistant mode**: always-on agents for each household user plus a
  shared Household agent, doing everyday tasks (email, calendar,
  browsing, documents, errands).
- **Operator mode**: the admin sets up and maintains sesh with the sesh
  admin CLI and Claude Code operator skills, which no agent can reach.

The design is described in the sesh Solutions Architecture document
(ADR 0001). These principles are binding on every spec, plan, task and
change. When a principle and a convenience conflict, the principle wins.

## Core Principles

### I. The control plane is the only trusted zone

The control plane (the gateway, the secret store, the approval channel,
the audit log) and the operator's host account are trusted. Everything
else is untrusted: every agent, the model's output, model-written
scripts **and the executor that runs them**, and all content an agent
reads (email, web pages, documents, other agents' requests). Authority
(scope, secrets, approvals, audit) is exercised only by the gateway,
whatever any other component reports. Isolation and approvals are
enforced in the control plane, never in prompts.

*Rationale*: any agent can be manipulated by content it reads at any
moment. The only boundary we can reason about is the one around the
control plane.

### II. One way out

Agents reach accounts, data, memory, documents and the browser only
through the gateway. Agents have no built-in terminal, file or web tools,
and no other network route to resources or the internet. Every allowance
is declared in operator configuration, scoped as narrowly as possible,
and inspectable.

*Rationale*: a single path is a path you can scope, approve and audit.

### III. Agents never see secrets

Credentials (OAuth tokens, API keys, site logins) live in the secret
store and are used by the gateway alone, in memory, never logged or
returned. Logins are filled into the browser by the gateway, never typed
by the model. New credentials reach the secret store through a one-time
form the person fills, never through an agent or a chat.

*Rationale*: a secret the agent can read is a secret it can leak.

### IV. Per-user isolation

Each user's agent can reach only that user's accounts, memory, documents
and skills. Shared accounts belong to the Household agent alone. The
gateway derives scope from the agent's token; the model never passes a
user or scope.

*Rationale*: one manipulated agent must not reach another person's data.

### V. Side effects need approval

Purchases, sends, deletes, settings changes and first logins to a site
pause for a human yes. The gateway writes the approval text itself and
delivers it on a channel no agent can use.

*Rationale*: the cost of a wrong action is bounded by a person who reads
exactly what will happen.

### VI. Agent-authored content never becomes trusted implicitly

Nothing an agent writes (skills, memory, notes, documents, downloads,
scripts) may widen what any agent can do, run in the control plane, or
enter this repository, except through a deliberate human step (review,
copy, approved pull request). Agent and household data live outside this
repository.

*Rationale*: self-learned skills and memories are instructions in
waiting; promoting them must be a human decision.

### VII. Local-first

All inference and household data stay on the home server. Exceptions are
named and minimal: Google APIs via OAuth, search engines behind the
search service, and content-free push wake-ups. No outbound telemetry
from sesh's own code.

*Rationale*: the household's data is the thing being protected.

### VIII. Adopt before build

sesh composes existing open-source components and builds only what no
project provides: the gateway, a few connectors, and glue. Each adopted
component is reached through exactly one module of sesh's code, so it can
be replaced. Features not needed by an accepted spec are not built.

*Rationale*: a small codebase is a small attack surface and a small
maintenance bill.

### IX. Swappable inference

Clients call model aliases (e.g. `agent-main`), never engines, so the
engine behind an alias can change without client changes. sesh works with
any inference server that speaks the OpenAI-compatible API and depends on
no particular engine (ADR 0005).

*Rationale*: local models and engines change faster than the platform.

### X. Every guarantee is executable

Each guarantee above has at least one automated check, including a
negative test for every isolation rule, and every phase ends with its
acceptance checks automated. A change that weakens or removes a check
must update this constitution in the same change.

*Rationale*: unverified guarantees decay.

## Additional Constraints

- **Platform**: the reference server is a Mac on Apple Silicon (built on
  an M5 Max MacBook Pro; long-term on a Mac mini or Studio, 64 GB target,
  peak about 45–50 GB). The inference server is the operator's choice and
  may run on bare metal when it needs the GPU; everything sesh runs is in
  containers, and work that touches untrusted web content runs in sbx
  microVMs.
- **Languages**: code sesh writes for the **trusted plane** (the gateway,
  the admin CLI, shared types) is **Rust** (stable, toolchain pinned,
  `unsafe` forbidden). Code that runs **inside an untrusted runtime** uses
  what that runtime requires: the adopted Code Mode runtime and thin
  connector stubs (JavaScript) inside the script executor, the Python
  memory plugin inside Hermes, the browser controller inside its sandbox
  (ADR 0002, ADR 0004). Minimal dependencies;
  anything heavier needs justification in a plan.
- **Repository boundaries**: this repository is the project monorepo:
  code, deployment templates, operator skills, specs and decisions. It
  holds no household data and no per-user or per-agent git repositories.
  Per-user and household repositories (agent skills, and any later
  per-user git integration) are separate, created by the admin CLI, and
  local to the server by default (ADR 0003).
- **No secrets in any repository**: `.env` holds only the secret store's
  bootstrap identity, injected at runtime.
- **Network**: all services listen on the container network or
  localhost; people reach sesh over Tailscale only.
- **Pinned minimum versions** for security-relevant components;
  `sesh doctor` refuses to run below them.

## Development Workflow

- Spec-driven: every feature goes through specify → clarify → plan →
  tasks → implement, under `specs/NNN-feature/`. The architecture's
  phases map to features.
- Every plan includes a **Constitution Check** that lists each principle
  and says whether the plan complies. Any exception is recorded with a
  justification in the plan's Complexity Tracking section.
- Test-first: a guarantee's check is written, and seen failing, before
  the code that provides it.
- Three test tiers:
  - **unit**: hermetic, with each adopted component faked at its module.
    Runs in CI.
  - **stack**: the Compose stack with a stub or small CPU model server in
    place of a real inference server; runs the isolation and approval
    negative tests. Runs in CI where containers are available.
  - **reference**: on the reference Mac, for what needs a real model or
    sbx (model quality, browser sessions).
- Decisions that change a principle, a component boundary, or the threat
  model are recorded as ADRs in `docs/decisions/`.

## Governance

This constitution supersedes other practices. Amendments require a pull
request that edits this file, explains the change, bumps the version, and
updates any affected checks and templates. Versioning:

- MAJOR: a principle is removed or weakened, or the product's scope
  changes.
- MINOR: a principle or constraint is added or materially expanded.
- PATCH: clarifications and wording.

All reviews check compliance with this constitution.

**Version**: 1.2.0 | **Ratified**: 2026-11-26 | **Last Amended**: 2026-11-26
