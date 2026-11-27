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

## Google connection (in the secret store; research.md "Build decisions")

| Key | Notes |
|---|---|
| `USER_<NAME>__GOOGLE__REFRESH_TOKEN` | the grant; the only secret |
| `USER_<NAME>__GOOGLE__SCOPES` | the space-separated scopes Google granted |
| `USER_<NAME>__GOOGLE__STATE` | `connected`, `revoked` or `removed` |

`<NAME>` is the owner in upper case, with `-` written as `_`; the
Household is `USER_HOUSEHOLD`. The OAuth client is `GOOGLE__CLIENT_ID`
and `GOOGLE__CLIENT_SECRET`, filled through the one-time credential form.

## Consent link (gateway memory)

| Field | Notes |
|---|---|
| `id` | 16 random bytes, hex |
| `owner` | |
| `secret_sha256` | of the 32-byte link secret, in the URL fragment |
| `state_sha256`, `pkce_verifier` | set at start; the OAuth `state` is single-use, with a 10-minute lifetime |
| link lifetime | 30 minutes; usable once |

A gateway restart forgets open links; the operator asks for a new one.

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
| upload types (fixed) | `application/pdf`, `image/png`, `image/jpeg`, `image/tiff` (Office files would need Tika and Gotenberg) |
| `ref_ttl_secs` | 3600 |

## Audit additions

| Event | `outcome` | `reason` |
|---|---|---|
| Consent link | `allowed` | `google.consent requested for <owner>` |
| Consent refused (bad, used or expired link) | `refused` | `google.consent refused: …` / `google.connect refused: …` |
| Google connected | `allowed` | `google.connected <owner> (<scope count>)` |
| Google disconnected | `allowed` | `google.disconnected <owner>` |
| Every mail, calendar, contacts or docs call | the call's outcome | includes `owner <owner>` |
