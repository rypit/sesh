# Data model: Foundation stack

No household data is modelled in this phase. These are sesh's own
configuration records; formats are in [contracts/config.md](./contracts/config.md).

## Config (`$SESH_DATA_DIR/sesh.toml`)

| Field | Type | Rules |
|---|---|---|
| `inference.base_url` | URL | Required. `http` or `https`; ends with the API root (e.g. `/v1`). Must not point at the router itself. |
| `inference.api_key_env` | env var name | Optional. Name of an environment variable holding the server's key; the value is never stored in sesh files. |
| `inference.timeout_secs` | integer | Default 120; 1–600. |
| `aliases.<alias>` | model name | At least `agent-main`. Alias names `[a-z0-9-]{1,40}`; model names non-empty. |
| `ports.webui` | port | Default 3000. |
| `ports.router` | port | Default 4000. |

Unknown keys are an error.

## Secrets (`$SESH_DATA_DIR/secrets.env`, mode 0600)

`POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `LITELLM_MASTER_KEY`,
`WEBUI_SECRET_KEY`, `WEBUI_ADMIN_PASSWORD` (generated, 32 random bytes,
URL-safe base64), and `WEBUI_ROUTER_KEY` (the restricted client key,
added by `sesh up` on first run). `WEBUI_ADMIN_EMAIL` is set by
`sesh init`. Values are never printed or logged.

## Rendered router config (`$SESH_DATA_DIR/litellm.yaml`)

Derived from Config only: one `model_list` entry per alias
(`model: openai/<model>`, `api_base: inference.base_url`, `api_key:
os.environ/<api_key_env>` or a placeholder), `general_settings.master_key:
os.environ/LITELLM_MASTER_KEY`, `database_url` from the environment. No
wildcard entries.

## Doctor check

| Field | Values |
|---|---|
| `id` | `alias-answers`, `alias-tools`, `ports-loopback`, `services-healthy`, `tailscale-serve` |
| `status` | `pass`, `fail`, `warn` |
| `detail` | what was observed |
| `fix` | what to do (on fail/warn) |

`sesh doctor` exits 0 when no check fails (warnings allowed), else 1.
