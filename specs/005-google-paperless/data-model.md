# Data model: Google and Paperless, with per-user identity

## User (`gateway/users.toml`)

| Field | Notes |
|---|---|
| `name` | `[a-z][a-z0-9-]{0,31}`; `household` is reserved and always present |
| `topic` | ntfy topic `sesh-<name>-<8 hex>`; Household = `gateway.ntfy_topic` |

## Agent (`gateway/agents.toml`, extends feature 002)

| Field | Notes |
|---|---|
| `owner` | a user name, or `household` (the default); must exist |

## Google connection (`gateway.google_connections`; the secret is in Infisical)

| Field | Notes |
|---|---|
| `user` | primary key |
| `scopes` | the space-separated scopes Google granted |
| `state` | `connected`, `revoked` or `removed` |
| `connected_at`, `updated_at` | |

Refresh token: Infisical key `USER_<NAME>__GOOGLE__REFRESH_TOKEN`, with
`-` in the name written as `_`.

## Consent link (`gateway.google_consents`)

| Field | Notes |
|---|---|
| `id` | 16 random bytes, hex |
| `user` | |
| `secret_sha256` | of the 32-byte link secret, in the URL fragment |
| `state_sha256`, `pkce_verifier` | set at start; the OAuth `state` is single-use, with a 10-minute lifetime |
| `expires_at` | 30 minutes |
| `used_at` | set when the start page is used; the callback also sets `finished_at` |

The PKCE verifier is a short-lived per-flow value, not a credential. It
is deleted at the callback.

## Attachment reference (gateway memory)

| Field | Notes |
|---|---|
| `ref` | 16 random bytes, hex |
| `agent` | only this agent may use it |
| `message_id`, `attachment_id`, `filename`, `mime`, `size` | |
| `expires_at` | 1 hour |

## Limits

| Setting | Default |
|---|---|
| `mail_results_max` | 25 |
| `upload_max_bytes` | 26_214_400 (25 MB) |
| `upload_types` | `application/pdf`, `image/png`, `image/jpeg`, `image/tiff`, the Office Open XML document, spreadsheet and presentation types, `application/msword` |
| `ref_ttl_secs` | 3600 |

## Audit additions

| Event | `outcome` | `reason` |
|---|---|---|
| Consent link | `allowed` | `google.consent.request <user>` |
| Google connected | `allowed` | `google.connected <user>` |
| Google disconnected | `allowed` | `google.disconnected <user>` |
| Google revoked | `refused` | `google.revoked <user>` |
| Every Google or docs call | the call's outcome | `owner <user>` |
| Upload | `allowed` | `docs.upload <doc id>` |
