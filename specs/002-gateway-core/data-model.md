# Data model: Gateway core

## Agent (`$SESH_DATA_DIR/agents.toml`)

| Field | Rules |
|---|---|
| `name` | `[a-z0-9-]{1,32}`, unique; names the executor service `executor-<name>` |
| `token_sha256` | hex SHA-256 of the token; the token itself only in `agents/<name>.token` (0600) |
| `connectors` | non-empty list of connector names known to the gateway |

## Connector descriptor (`sesh-core`)

| Field | Rules |
|---|---|
| `name` | `[a-z][a-z0-9_]{0,31}`; the global name in the sandbox |
| `instructions` | one or two sentences for the model |
| `methods.<name>.description` | required |
| `methods.<name>.input` | JSON Schema subset: object, string (maxLength), integer/number (min/max), boolean, array (items, maxItems), enum, required |
| `methods.<name>.requires_approval` | bool; gated methods are refused until Phase 5 |

## Execution (gateway memory)

| Field | Rules |
|---|---|
| `id` | `exec_` + 16 random bytes hex |
| `agent` | from the token |
| `capability` | 32 random bytes, URL-safe base64; valid until `expires_at` |
| `expires_at` | start + deadline + 5 s |
| `calls` | count; limit 50 (configurable) |

## Audit entry (Postgres `gateway.audit`)

| Column | Type | Rules |
|---|---|---|
| `id` | bigserial | |
| `at` | timestamptz | default now() |
| `agent` | text | |
| `execution` | text | null for refusals before an execution exists |
| `connector`, `method` | text | null when not applicable (e.g. unknown token) |
| `args_summary` | jsonb | keys, types, strings cut to 40 chars; never tokens or capabilities |
| `outcome` | text | `allowed`, `refused`, `failed` |
| `reason` | text | refusal or failure reason |

Grants: role `sesh_gateway` has `INSERT` on `audit` and `USAGE` on its
sequence only.

## Gateway settings (`sesh.toml [gateway]`)

| Field | Default |
|---|---|
| `deadline_secs` | 30 |
| `max_calls` | 50 |
| `max_result_bytes` | 65536 |
| `home.latitude`, `home.longitude` | required for `geo` (set by `sesh init --home LAT,LON`; tests use a fixed point) |
| `upstreams.searxng` | `http://searxng:8080` |
| `upstreams.kiwix` | `http://kiwix:8080` |
| `upstreams.open_meteo` | `https://api.open-meteo.com` |
| `upstreams.nominatim` | `https://nominatim.openstreetmap.org` |
