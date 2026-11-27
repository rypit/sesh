# Implementation Plan: Gateway core

**Branch**: `002-gateway-core` | **Date**: 2026-11-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/002-gateway-core/spec.md`

## Summary

`sesh-gateway` (Rust) is the MCP server agents reach with their token.
It offers `execute`, `search` and `describe`. Scripts run in a per-agent
executor container (workerd + the adopted Code Mode runtime, behind a
small Rust supervisor that enforces deadlines), which forwards every
connector call back to the gateway with a short-lived capability. The
gateway checks the capability, the agent's allowlist and the method's
input, refuses approval-gated methods, calls the upstream, writes the
audit log (insert-only), and returns results marked untrusted. First
connectors: `search` (SearXNG, optional Kiwix) and `geo` (Open-Meteo,
Nominatim).

## Technical Context

**Language/Version**: Rust (pinned toolchain) for the gateway, supervisor
and CLI additions; JavaScript (bundled) for the executor's host Worker
and connector stubs (ADR 0002).

**Primary Dependencies**: research.md R8. Executor: `@cloudflare/codemode`
0.5.6, `workerd` (pinned), `esbuild` at build time.

**Storage**: Postgres database `gateway` (audit); `agents.toml` and
per-agent token files in `$SESH_DATA_DIR`.

**Testing**: unit tests per crate (connectors against in-process fakes,
capability and allowlist rules, schema validation, TS generation,
argument redaction); stack tests extend `crates/sesh-cli/tests/stack.rs`
with a test MCP client, hostile scripts, and the fake-upstreams service.

**Target Platform**: reference Mac (Docker Desktop, arm64) and Linux CI.

**Project Type**: Cargo workspace + container images.

**Performance Goals**: SC-001 (two calls < 2 s), SC-004 (runaway stopped
within 5 s of deadline; other agent < 2 s).

**Constraints**: executors have no internet and no secrets; only the
gateway reaches upstreams; nothing secret in logs or results.

**Scale/Scope**: a handful of agents.

## Constitution Check

*GATE: before Phase 0 and after Phase 1. Both pass.*

| Principle / constraint | How this plan complies |
|---|---|
| I. Control plane only trusted zone | Executor and supervisor treated as untrusted; gateway re-checks capability, allowlist, input, approval. |
| II. One way out | Agents reach only the gateway's agent listener; executors only its executor listener; upstreams only from the gateway. |
| III. Agents never see secrets | No secrets in executors; capabilities are per execution and useless outside the executor listener; tokens stored hashed. |
| IV. Per-user isolation | Scope from token; per-agent executor and network. |
| V. Side effects need approval | Gated methods refused until Phase 5's approval channel. |
| VI. Agent-authored content never trusted | Scripts and results are data; upstream content labelled untrusted. |
| VII. Local-first | Search and encyclopedia local; weather and maps are named keyless public services, called only by the gateway. |
| VIII. Adopt before build | Adopts Code Mode runtime, workerd, SearXNG, kiwix-serve; builds the gateway, supervisor, connectors. |
| IX. Swappable inference | Not touched. |
| X. Every guarantee executable | Negative tests for every refusal, audit immutability, deadline, isolation (FR-017). |
| Languages | Gateway, supervisor, CLI in Rust; executor JS only inside workerd. |
| Repository boundaries | Agent tokens and records in `$SESH_DATA_DIR`. |

## Project Structure

### Documentation (this feature)

```text
specs/002-gateway-core/
├── spec.md · plan.md · research.md · data-model.md · quickstart.md
├── contracts/
│   ├── mcp.md        # agent-facing tools
│   ├── executor.md   # gateway ↔ executor protocol
│   ├── connectors.md # search and geo methods
│   └── cli.md        # sesh agent add/remove, doctor additions
└── tasks.md
```

### Source Code

```text
crates/
├── sesh-core/                    # shared types: agent records, connector descriptors, audit entry
├── sesh-gateway/
│   └── src/
│       ├── main.rs               # two listeners: agents (:7000), executors (:7001)
│       ├── mcp.rs                # rmcp ServerHandler: execute, search, describe
│       ├── auth.rs               # token middleware (hash lookup)
│       ├── capability.rs         # issue/check/expire
│       ├── executor.rs           # client for POST /execute with deadline
│       ├── calls.rs              # executor-facing /call: checks, dispatch, audit
│       ├── schema.rs             # validator + TypeScript declarations
│       ├── audit.rs              # seam: the only module writing Postgres
│       ├── redact.rs             # argument summaries
│       └── connectors/{mod,search,geo}.rs  # one module per upstream
├── sesh-executor-supervisor/     # runs workerd, enforces deadlines
├── sesh-cli/                     # + agent add/remove, migration, doctor checks
└── sesh-stub-llm/                # + fake upstream routes (SearXNG, Kiwix, Open-Meteo, Nominatim)
executor/
├── package.json, package-lock.json
├── src/host.js                   # host Worker + Durable Object runtime + connector stubs
└── workerd.capnp
deploy/
├── compose.yaml                  # + gateway, searxng; kiwix (profile)
├── gateway.Dockerfile, executor.Dockerfile
├── searxng/settings.yml
└── postgres/init.sql             # + gateway db (fresh installs)
```

`sesh up` renders `compose/agents.yaml`: one `executor-<agent>` service
and one internal network `exec-<agent>` per agent, both joined by the
gateway.

## Key flows

**execute**: agent → gateway `:7000/mcp` (token) → new execution +
capability → `POST executor-<agent>:8000/execute` (code, capability,
the agent's connector descriptors, deadline) → supervisor → workerd
runtime → each stub call → `POST gateway:7001/call` (capability) →
checks → upstream → audit → result → script result → gateway truncates,
labels untrusted, returns. On deadline: supervisor kills and restarts
workerd; gateway returns a timeout error and drops the capability.

## Risks

| Risk | Mitigation |
|---|---|
| Code Mode runtime API changes (0.5.x) | Pinned; executor tests in the stack suite. |
| kiwix/Nominatim response shapes differ from fakes | Reference-tier check; connectors tolerant parsers. |
| Restart leaves a paused Code Mode execution | Phase 2 has no approvals; Phase 5 revisits resumption. |
| Per-agent networks grow the gateway's network list | Fine for a household; documented. |

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| Supervisor process in the executor | Local workerd has no CPU limits; restart without the Docker socket | Giving the gateway the Docker socket grants root-equivalent host access |
