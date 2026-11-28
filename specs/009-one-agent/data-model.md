# Data model: One agent end to end

## Agent record (`gateway/agents.toml`, extended)

| Field | Type | Notes |
|---|---|---|
| `name` | string | as today |
| `owner` | string | as today (`household` in this feature) |
| `connectors` | list | as today |
| `token_sha256` | string | as today |
| `runtime` | optional string | new: `"hermes"`, or absent for an identity-only agent (tests, the eval) |

The gateway ignores `runtime`; it is read by `sesh up` and the doctor.

## Agent runtime (derived by `sesh up`, not stored)

| Item | Value |
|---|---|
| Service | `hermes-<name>` |
| Image | R1's pin |
| Networks | `agent-<name>`, `agent-<name>-model`, `agent-<name>-chat` (all internal) |
| State | named volume `hermes-<name>-data` at `/opt/data` |
| Config | `$SESH_DATA_DIR/agents/<name>/config.yaml` → `/opt/data/config.yaml:ro` |
| Credentials | `$SESH_DATA_DIR/agents/<name>.env` (0600) as `env_file` |
| Listens | `:8642` |
| Model name in Open WebUI | `<name>` |

## Agent network (dynamic layout entries)

`layout.rs` gains `agent_nets(name) -> Vec<DynNet>` returning the three
networks above with their members, and `matrix_with(stub, agents)`,
which adds the runtime's listener (`hermes-<name>`, 8642), its used
paths and the gateway's `:7000` on `agent-<name>` only.

Used paths per agent:
- `hermes-<name>` → `gateway:7000` (on `agent-<name>`)
- `hermes-<name>` → `litellm:4000`
- `open-webui` → `hermes-<name>:8642`

## Gateway settings (`gateway.toml`, extended)

| Field | Type | Notes |
|---|---|---|
| `agent_listen` | list of hostnames | new: `gateway-agent-<name>` per runtime; resolved at start; a failure stops the gateway |

## Secrets (`secrets.env`, new keys)

| Key | Created by | Used by |
|---|---|---|
| `AGENT_<NAME>_API_KEY` | `sesh agent add --runtime` | the runtime (requires) and Open WebUI (sends) |
| `AGENT_<NAME>_ROUTER_KEY` | `sesh up` (LiteLLM, aliases only) | the runtime |

`<NAME>` is the agent name upper-cased with `-` as `_`. `sesh agent
remove` deletes both and the router key is revoked on the next `sesh up`.
