# Implementation Plan: One agent end to end

**Branch**: `claude/one-agent` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/009-one-agent/spec.md`

## Summary

- **The runtime:** Hermes Agent's official image, pinned (research.md
  R1), one container per agent with a runtime (`hermes-<name>`).
- **Tools:** only the gateway's. `platform_toolsets.api_server:
  [mcp-sesh]`, and every built-in toolset in `agent.disabled_toolsets`
  (R2). The doctor and the stack test check the runtime's resolved
  toolsets and what the model is actually offered.
- **Model:** LiteLLM's `agent-main`, with the agent's own router key,
  limited to the aliases (R3).
- **Chat:** Open WebUI gets one OpenAI-compatible connection per agent
  (R4), keyed with the agent's API key.
- **Networks:** three internal pair networks per agent: `agent-<name>`
  (with the gateway), `agent-<name>-model` (with LiteLLM),
  `agent-<name>-chat` (with Open WebUI). No edge network, so no route
  out (R6).
- **Gateway listener:** `:7000` also binds the gateway's own address on
  each `agent-<name>` network, found by resolving its network alias
  `gateway-agent-<name>` (R5). Never `0.0.0.0`, never `exec-*`.
- **Credentials:** a per-agent env file (0600) with the gateway token,
  the router key and the API key; nothing in the Compose files (R7).
  `config.yaml` mounted read-only (R8).
- **CLI:** `sesh agent add … --runtime hermes`; `sesh up` generates the
  runtime, its networks and keys; `sesh doctor` gains `agent-runtimes`.
- **Tests:** `agent_stack` drives a scripted conversation (the stub's
  `STUB_SCRIPT=agent`, R10) through Open WebUI and the runtime to the
  gateway, checks the offered tools, the refused `terminal` call, the
  gated send and the container's reachability.
- **ADR 0012**, architecture text and diagram.

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002), with
`#![forbid(unsafe_code)]`; Compose YAML; Hermes's YAML config generated
by sesh.

**Primary Dependencies**: Hermes Agent v0.23.4 (new adopted
component). No new Rust crates.

**Storage**: one named volume per agent (`hermes-<name>-data`).

**Testing**:
- `cargo test`: the config renderer (toolsets off, MCP only, model),
  the overlay (service, networks, alias, Open WebUI connections), the
  gateway's alias binding, the layout's agent networks, the stub's
  agent script, the doctor's check.
- `make stack-test`: a new `agent_stack`; `network_stack` with one
  runtime.

**Target Platform**: Linux with Docker; the Mac mini later (ADR 0001).
The image is multi-arch.

**Project Type**: a Compose stack and its admin CLI.

**Performance Goals**: none new; the runtime idles at a few hundred MB.

**Constraints**:
- CI runs with containers' internet blocked; the image is pulled by the
  runner.
- No secret value in any generated file but the 0600 env files and
  `secrets.env`.

**Scale/Scope**: one runtime (the Household agent) in this feature; the
design is per agent.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check |
|---|---|
| I. Only the control plane is trusted | The runtime is untrusted: its own networks only, no route out, read-only config, no privilege escalation. |
| II. One way out | **Central.** Built-in tools off twice over (R2); the only network path to a resource is the gateway's `:7000` on `agent-<name>`. |
| III. Secrets | The runtime holds its gateway token and keys that open only the router's aliases and its own API (R7). No resource credential. |
| IV. Per-user isolation | Unchanged: the gateway derives scope from the token's owner. The Household agent's owner is `household`. |
| V. Approvals | Unchanged: `mail.send` is gated in the gateway; the test checks it through the runtime. |
| VI. Agent content is data | The runtime's config is written by sesh and mounted read-only; nothing it writes is read back as configuration. |
| VII. Local-first | The runtime has no route out; inference is the household's own via LiteLLM. Its vendor's hosted services are unreachable. |
| VIII. Adopt before build | Hermes is adopted, unmodified, from its official image. It is reached through one module, `crates/sesh-cli/src/hermes.rs` (AGENTS.md). |
| IX. Swappable inference | The runtime uses the `agent-main` alias, never a model name. |
| X. Every guarantee is executable | Each guarantee (tools, reachability, credentials, approval) has a stack or unit test, written failing first. |

Passes. A new adopted component and a listener change: ADR 0012.

## Project Structure

```text
crates/sesh-core/src/lib.rs           Agent.runtime
crates/sesh-cli/src/agent.rs          --runtime, API key
crates/sesh-cli/src/hermes.rs         new: config.yaml, env file, service block
crates/sesh-cli/src/hermes/toolsets.txt  the built-in toolsets reference
crates/sesh-cli/src/up.rs             overlay, router keys, Open WebUI connections, agent_listen
crates/sesh-cli/src/layout.rs         agent networks
crates/sesh-cli/src/doctor.rs         agent-runtimes
crates/sesh-gateway/src/listen.rs     alias binding
crates/sesh-gateway/src/main.rs       bind on agent_listen
crates/sesh-stub-llm/src/main.rs      STUB_SCRIPT=agent, /stub/requests
crates/sesh-cli/tests/stack.rs        agent_stack
deploy/compose.yaml                   Open WebUI's connection list
docs/decisions/0012-agent-runtime.md
docs/architecture/…                   text and diagram
```

## Complexity Tracking

None beyond the ADR.
