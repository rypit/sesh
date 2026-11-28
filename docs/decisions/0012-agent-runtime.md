# 0012. Hermes as the agent runtime, reaching only the gateway

- Status: accepted
- Date: 2026-11-27
- Feature: specs/009-one-agent

## Context

Until now sesh's agents were identities: test clients and the stub
model called the gateway with an agent's token. The architecture names
Hermes Agent as the runtime household members chat with through Open
WebUI. Hermes ships with many built-in tools (a terminal, files, a web
fetcher, a browser, code execution, image generation, memory), any of
which would be a way out around the gateway (constitution II). And
since ADR 0011 the gateway's agent listener (`:7000`) reaches no
container at all.

## Decision

1. **Hermes Agent, from its official image pinned by digest**, one
   container per agent that has a runtime (`hermes-<agent>`), configured
   entirely by sesh. Agents without a runtime stay identities.
2. **Only the gateway's tools.** The API server platform's toolsets are
   exactly the gateway's MCP server (`mcp-sesh`), and every built-in
   toolset is also disabled globally. `sesh doctor` and the stack test
   check the runtime's resolved toolsets and the tools the model is
   actually offered.
3. **Three internal pair networks per agent, no route out**:
   `agent-<agent>` (with the gateway), `agent-<agent>-model` (with
   LiteLLM) and `agent-<agent>-chat` (with Open WebUI). No edge network
   and no published port.
4. **The gateway's agent listener also binds on each `agent-<agent>`
   network**: `gateway.toml` lists the gateway's alias on each
   (`gateway-agent-<agent>`), and the gateway binds `:7000` on the
   address the alias resolves to. It still never binds `0.0.0.0`, the
   approvals listener is unchanged, and executor networks are never
   listed. An alias that doesn't resolve stops the gateway.
5. **Credentials**: the agent's gateway token, its own router key
   (aliases only) and its API key, in a per-agent 0600 env file. The
   Compose files carry no values. The configuration file is written by
   sesh and mounted read-only.
6. **Open WebUI connects to each runtime** as an OpenAI-compatible
   connection with the agent's API key. In this feature only the
   Household agent runs, visible to every household member; per-user
   visibility comes with feature 012.

## Alternatives

- **No runtime; Open WebUI's own tool calling against the gateway**:
  Open WebUI isn't an agent loop, and its tools live in its database
  (constitution VI). Rejected.
- **Bind `:7000` on all interfaces with a peer check** (as `:7001`
  does): a check would be the only guard, on every network. Rejected.
- **One shared network per agent** for the runtime, the gateway,
  LiteLLM and Open WebUI: re-opens `litellm → gateway:7000`. Rejected.
- **A patched or self-built image** without the built-in tools: more
  to maintain; configuration plus network isolation gives two
  independent guards. Rejected.

## Consequences

- **Threat model:** a compromised runtime (prompt injection, a hostile
  model) can call the gateway with its own token, the router with an
  aliases-only key, and nothing else: no internet, no other service.
  Pair networks leave reverse paths (the runtime to Open WebUI's
  `:8080` and to the gateway's `:7001`); the first needs a login, the
  second refuses every caller but the agent's own executor (ADR 0009).
- Hermes's own update checks and hosted services can't be reached.
- Each runtime adds about 1.4 GB of image (shared) and a few hundred MB
  of memory.
- Adding a runtime is `sesh agent add … --runtime hermes` and `sesh up`.
