# Data model: Browser, with a live view and takeover in the chat

## Browser session (gateway memory)

| Field | Notes |
|---|---|
| `agent` | one session per agent |
| `cdp` | the CDP connection to `browser-<agent>:9222` |
| `control` | `agent` or `person { grant, since }` |
| `last_used` | for idle shutdown of the screencast |

## Live view grant (`live_grants` table, gateway database)

| Column | Type | Notes |
|---|---|---|
| `id` | text | URL path part |
| `secret_sha256` | text | the URL fragment's hash; the secret is never stored |
| `agent` | text | |
| `webui_user` | text | Open WebUI user id from the signed header |
| `created_at`, `last_used_at` | timestamptz | expires 30 min after last use |

## Approved site (`browser_sites` table)

| Column | Type | Notes |
|---|---|---|
| `owner` | text | the agent's owner |
| `site` | text | host the credential was saved for |
| `approved_at` | timestamptz | first login approved |

## Site login (secret store)

`browser` connector credential `SITE_<host>`, value
`{"username":…, "password":…, "totp":…?}` (JSON), written by the form,
read only by the gateway.

## Gateway settings (`gateway.toml`, new)

| Field | Default | Notes |
|---|---|---|
| `browser_read_max_bytes` | 51200 | |
| `browser_read_timeout_secs` | 20 | |
| `browser_wait_secs` | 60 | agent waits while a person is in control |
| `browser_takeover_max_secs` | 900 | |
| `live_grant_ttl_secs` | 1800 | |
| `egress_allow` | `[]` | stub mode only: the stub's addresses |
| `chat_upstreams` | | agent → `http://hermes-<agent>:8642` |

## Networks (layout.rs)

- `browser-<agent>` (internal): `browser-<agent>`, the gateway (CDP 9222
  one way; the egress proxy `:7004` the other)
- `chat` (internal): Open WebUI and the gateway (chat listener `:7003`)
- removed: `agent-<agent>-chat`

## File reference (gateway memory, `refs.rs`)

| Field | Notes |
|---|---|
| `ref` | 32 hex characters, issued to one agent |
| `source` | `GmailAttachment {message, attachment}`, `Download {store id}`, or `Document {paperless id}` |
| `filename`, `type`, `size` | as shown to the agent and in approvals |
| `expires` | 24 hours (downloads) or as today (attachments) |

## Download store (gateway)

Bytes per download: in memory up to 1 MB, otherwise in a gateway-owned
directory (`/var/lib/sesh-gateway/downloads`, tmpfs-backed volume),
deleted when the reference expires. At most 20 per agent.
