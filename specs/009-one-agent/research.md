# Research: One agent end to end

Sources: Hermes Agent's repository (NousResearch/hermes-agent, MIT): its
README, Dockerfile, `docker-compose.yml`, `cli-config.yaml.example`, and
the API server, configuration, MCP, tools, toolsets and Docker pages of
its documentation, read from raw GitHub on 2026-11-27 (the docs site is
not reachable from the build environment).

## R1. The runtime and its image

**Decision**: Hermes Agent from its official image,
`nousresearch/hermes-agent:v0.23.4@sha256:ef0650d523ee1b5ab32c779acabc446d9936869a9932591553688a62bcf0c42f`
(multi-arch index, released 2026-11-24, about 1.4 GB).

**Rationale**: The architecture names Hermes as the agent runtime. The
official image is maintained upstream and multi-arch (the Mac mini is
arm64). Pinning version and digest follows every other adopted image.

**Alternatives**: building our own image from source (more to maintain,
no gain); an older release (v0.23.3 `sha256:3e9fe4c6…`, v0.23.2
`sha256:d6746a08…`) is the fallback if v0.23.4 fails in CI.

**Container facts**: all state is under `/opt/data` (`config.yaml`,
`.env`, sessions); the entrypoint is s6-overlay's `/init` (PID 1, must
stay so); the process runs as the non-root `hermes` user
(`HERMES_UID`/`HERMES_GID`, default 10000) after s6's setup as root;
`/opt/hermes` is root-owned and read-only to that user.

## R2. Turning every built-in tool off

**Decision**: in the `config.yaml` sesh writes:

```yaml
platform_toolsets:
  api_server: [mcp-sesh]     # the gateway's tools, nothing else
agent:
  disabled_toolsets: [<every built-in toolset>]
mcp_servers:
  sesh:
    url: http://gateway:7000/mcp
    headers: {Authorization: "Bearer ${SESH_GATEWAY_TOKEN}"}
```

`platform_toolsets.api_server` limits the API server platform to the
named toolsets; an MCP server becomes the dynamic toolset `mcp-<name>`.
`agent.disabled_toolsets` removes toolsets on every platform after the
platform list, so it is a second, independent off switch. The list
names every built-in toolset in the toolsets reference (web, search,
terminal, file, browser, vision, image_gen, video_gen, skills, tts,
todo, memory, session_search, cronjob, code_execution, delegation,
clarify, messaging, connections, computer_use, kanban, x_search,
catalog and the rest); a unit test pins it against the reference copy
in `crates/sesh-cli/src/hermes/toolsets.txt`.

**Verification**: the runtime's `GET /v1/toolsets` returns the toolsets
resolved for the `api_server` platform, with their tools. The doctor
and the stack test require exactly one enabled toolset, `mcp-sesh`. The
stack test also checks the tool list in the request the stub model
receives (the ground truth: what the model is offered).

**Alternatives**: a custom image with the tools removed (patching the
runtime, rejected); trusting the platform list alone (one switch;
rejected for defence in depth).

## R3. The model

**Decision**: `model.provider: custom`, `model.base_url:
http://litellm:4000/v1`, `model.default: agent-main`, the key from
`${SESH_ROUTER_KEY}`; every auxiliary task (compression, titles,
vision) set to `main`, so nothing tries another provider. The router key
is the agent's own, created by `sesh up` and limited to the aliases,
like Open WebUI's and the eval's.

## R4. Open WebUI to the runtime

**Decision**: Open WebUI gets one more OpenAI-compatible connection
per agent: `OPENAI_API_BASE_URLS` and `OPENAI_API_KEYS`
(semicolon-separated), replacing the single `OPENAI_API_BASE_URL`. The
first connection stays the router (its aliases); the runtime's
connection is `http://hermes-<agent>:8642/v1` with the agent's own API
key. `ENABLE_PERSISTENT_CONFIG` is already `False`, so the environment
wins on every start. The runtime's `/v1/models` lists one model, named
after the profile (`hermes-agent` for the default profile); sesh sets
the profile name to the agent's name so it is listed as, for example,
`household-agent`.

The runtime's API server requires `API_SERVER_KEY` as a bearer token
and binds `API_SERVER_HOST` (`0.0.0.0` in its container, where only its
pair networks reach it). CORS stays off. Tool-progress events stay on:
Open WebUI shows them as status and the browser feature (010) builds on
them.

**Alternatives**: routing the runtime through LiteLLM as a "model"
(LiteLLM would hold the runtime's key and every agent would be visible
to every router client; rejected); an Open WebUI pipe function (code in
Open WebUI's database; rejected, constitution VI).

## R5. How the runtime reaches the gateway (the listener rule)

Since 008 the gateway's agent listener (`:7000`) binds loopback and its
default-route address (`gateway-edge`) only, so no container reaches
it. The runtime must.

**Decision**: a per-agent internal network `agent-<name>`, with only
the runtime and the gateway on it. The gateway gets a network alias
there, `gateway-agent-<name>`, and `gateway.toml` lists the agent
networks' aliases (`agent_listen = ["gateway-agent-<name>"]`). At start
the gateway resolves each alias through Docker's DNS, which answers
with its own address on that network only, and also binds `:7000`
there. `:7002` (approvals) is unchanged. `exec-*` networks are never
listed, so executors still can't reach `:7000`.

**Rationale**: binding stays exact (no `0.0.0.0`); sesh knows the
aliases from `agents.toml` before anything starts, so there's no
ordering problem with reading subnets; an alias that doesn't resolve is
a start-up error (fail closed), so a mis-wired gateway never silently
binds more.

**Alternatives**:
- Bind `0.0.0.0:7000` and check the peer's address (like `:7001`):
  would open `:7000` on every network, with a check as the only guard.
  Rejected.
- Write the networks' subnets into `gateway.toml`: needs the networks
  to exist before the gateway's config is written; rejected for the
  alias, which needs nothing.

## R6. Networks: three pair networks per agent

**Decision**: each runtime is on exactly three internal networks, each
a pair:
- `agent-<name>`: the runtime and the gateway (the runtime calls `:7000`);
- `agent-<name>-model`: the runtime and LiteLLM (`:4000`);
- `agent-<name>-chat`: Open WebUI and the runtime (`:8642`).

No edge network: the runtime has no route out and no published port.

**Rationale**: one shared network for all four would put LiteLLM and
Open WebUI next to the gateway's agent listener, re-opening the
`litellm → gateway:7000` path 008 closed. Pairs keep every link
explicit. The reverse paths that pairs leave open (the gateway to
`:8642`, LiteLLM to `:8642`, the runtime to Open WebUI's `:8080`, the
runtime to the gateway's `:7001`) are listed in the layout table:
`:8642` and `:8080` need keys the other side doesn't have, and `:7001`
refuses every caller but the agent's own executor (ADR 0009).

## R7. Credentials

**Decision**:
- **The gateway token**: the agent's existing one (`agents/<name>.token`,
  0600).
- **The router key** (`AGENT_<NAME>_ROUTER_KEY`) and **the API key**
  (`AGENT_<NAME>_API_KEY`) live in `secrets.env`, created by `sesh up`
  (router key) and `sesh agent add` (API key). The API key is shared by
  the runtime (which requires it) and Open WebUI's connection (which
  sends it).
- `sesh up` writes `agents/<name>.env` (0600) with `SESH_GATEWAY_TOKEN`,
  `SESH_ROUTER_KEY` and `API_SERVER_KEY`; the runtime's service takes it
  as `env_file`. The generated Compose file holds no value, only the
  path. Open WebUI's keys are interpolated from `secrets.env` as today.

**Constitution III**: the runtime holds its gateway token and two keys
that open only the model router's aliases and its own API; no resource
credential. Nothing prints a value.

## R8. Configuration the agent can't change

**Decision**: `config.yaml` is written by sesh to
`agents/<name>/config.yaml` and bind-mounted read-only at
`/opt/data/config.yaml`; the rest of `/opt/data` is a per-agent named
volume (`hermes-<name>-data`). The runtime has no tool that could write
anything (R2), and its configuration file isn't writable even if it had.
If the runtime refuses to start with a read-only `config.yaml`, the
fallback is to copy it in on each start (s6 `cont-init` is not ours to
change; a sesh-owned init container would do it) and check it in the
doctor; recorded here if needed.

## R9. Hardening

**Decision**: `cap_drop: [ALL]` plus the capabilities s6's setup needs
to drop to the `hermes` user (`CHOWN`, `SETUID`, `SETGID`,
`DAC_OVERRIDE`, `FOWNER`), `no-new-privileges`, `mem_limit: 2g`,
`pids_limit: 512`; `read_only` root filesystem with `tmpfs` for `/run`
and `/tmp` if s6 allows it, else recorded as a follow-up. The healthcheck
is the API server's `GET /health`.

## R10. The stub model plays the runtime's conversation

The gateway exposes three MCP tools (code mode, feature 002): `search`,
`execute` and `describe`. The runtime prefixes MCP tool names, so the model sees
them as something like `mcp_sesh_execute`.

**Decision**: `STUB_SCRIPT=agent` (contracts/stub-script.md) adds a
script keyed by an `[agent:<step>]` tag in the latest user message,
reusing the eval script's `execute` code:
- `weather`: `execute` with `geo.forecast({})`;
- `search`: `execute` with `search.web({query:"varden beacon"})`;
- `mail`: `execute` with the eval's `mail.search` + `mail.read`;
- `feeds`: `execute` with `feeds.latest({feed:"harbour"})`;
- `send`: `execute` with the eval's `mail.send` (gated);
- `terminal`: a call to a tool named `terminal` with `{"command":"id"}`,
  which the runtime doesn't offer.

After a tool message it answers quoting the tool text (so the fakes,
the gateway and the runtime are all exercised). It picks the offered
tool whose name ends in `execute`, so the test doesn't depend on the
runtime's prefix. It records each request's offered tool names,
readable at `GET /stub/requests`.

## R11. CI

**Decision**: a new `agent_stack` test, in its own CI group (the image
is large; pulling it in a group with other heavy tests would slow
them). It needs the image pulled with the internet allowed for the
runner, as the other adopted images are.

## R12. Findings from running v0.23.4 (2026-11-28)

Run locally on an internal network with a fake gateway (MCP over HTTP)
and a fake model, using the configuration `hermes.rs` writes:

- **Tool search bridge.** By default Hermes hides MCP tools behind three
  bridge tools (`tool_search`, `tool_describe`, `tool_call`).
  `tools.tool_search.enabled: off` makes the model see exactly the
  gateway's tools, named `mcp__sesh__<tool>` (the fake gateway offered
  two; the real one offers `search`, `execute` and `describe`, and
  `agent_stack` compares against its `tools/list`). The stub's suffix
  match (R10) holds.
- **Built-ins are refused.** A model call to `terminal` gets "Tool
  'terminal' does not exist. Available tools: mcp__sesh__execute,
  mcp__sesh__search", and nothing runs.
- **`/v1/toolsets`** lists only built-in toolsets (26 of them, `stt` and
  `a2a` beyond the docs' table, now in `toolsets.txt`), every one
  `enabled: false`. MCP toolsets aren't listed, so the doctor checks
  that no listed toolset is enabled, and the stack test checks the tools
  the model is offered (contracts/cli.md updated).
- **The router key** is read from `model.key_env: SESH_ROUTER_KEY`.
  `OPENAI_API_KEY` and an interpolated `model.api_key` were ignored, and
  a key that looks like a placeholder (too short) is replaced by
  `no-key-required`; LiteLLM's `sk-…` keys are used as given.
- **The MCP token** is interpolated in `mcp_servers.sesh.headers` from the
  environment, as documented. If the gateway is unreachable at start,
  Hermes parks the server until a reconnect; `depends_on: gateway:
  service_healthy` keeps that from happening at `sesh up`.
- **Read-only `config.yaml` works.** Hermes's start-up schema migration
  fails to write it, logs the error, and continues; the configuration
  is used as written (R8 holds, no fallback needed).
- **Hardening works** (R9): a read-only root filesystem with tmpfs
  `/run` (exec) and `/tmp`, `cap_drop: ALL` plus `CHOWN`,
  `DAC_OVERRIDE`, `FOWNER`, `SETGID`, `SETUID`, and `no-new-privileges`.
  The agent runs as `hermes`; only s6's init runs as root.
- **Listeners**: `0.0.0.0:8642` (the API server, 401 without its key) and
  one loopback-only port (Hermes's internal); nothing else.
- **Model name**: `API_SERVER_MODEL_NAME` sets the `/v1/models` id.
- Hermes couldn't determine the alias's context length and assumes
  256k tokens. Follow-up: set `model.context_length` from the router's
  model info once real hardware is in use.
