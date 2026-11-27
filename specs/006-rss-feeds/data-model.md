# Data model: RSS feeds

## Feed (gateway Postgres, table `feeds`)

| Field | Notes |
|---|---|
| `owner` | a user name, or `household`; must exist when added |
| `name` | `[a-z0-9][a-z0-9-]{0,31}`; unique per owner |
| `url` | http or https, a host, no credentials, ≤ 2,000 characters |
| `added_by` | `operator`, or the agent's name (an approved add) |
| `added_at` | Unix seconds |

Primary key `(owner, name)`. `sesh_gateway` may `SELECT`, `INSERT` and
`DELETE`. At most `feeds_max_per_owner` rows per owner.

## Feed copy (gateway memory, by URL)

| Field | Notes |
|---|---|
| `fetched_at` | when it was fetched |
| `outcome` | the parsed feed, or the failure's reason |

Used for `feeds_cache_secs` (a failure for 60 s). The last outcome per
URL is kept for doctor: `ok`, `failed` (with the reason) or `never`.

## Parsed feed

| Field | Notes |
|---|---|
| `title` | plain text, ≤ 300 characters |
| `items` | ≤ 200, in feed order |

## Feed item (what agents get)

| Field | Notes |
|---|---|
| `title` | plain text, ≤ 300 characters |
| `link` | http(s) only, else empty; ≤ 2,000 characters |
| `published` | RFC 3339 UTC when readable, else as written (≤ 64), or empty |
| `summary` | plain text, markup removed, ≤ 1,000 characters |

## Gateway settings (`GatewaySettings`, `gateway.toml`)

| Setting | Default | Notes |
|---|---|---|
| `feeds_cache_secs` | 900 | 60–86,400 |
| `feeds_max_bytes` | 2,097,152 | 1 KiB–16 MiB |
| `feeds_timeout_secs` | 10 | 1–60 |
| `feeds_max_per_owner` | 50 | 1–1,000 |
| `feeds_allow_hosts` | `[]` | single-label names only; honoured only with test connectors (research.md R4) |

Fixed: 3 redirects; caps in research.md R6.

## Audit additions

| Event | `agent` | `outcome` | `reason` |
|---|---|---|---|
| Operator adds a feed | `operator` | `allowed` | `feed.added <owner>/<name>` |
| Operator removes a feed | `operator` | `allowed` | `feed.removed <owner>/<name>` |
| Operator add refused | `operator` | `refused` | `feed.add refused: <why>` |
| Every `feeds.*` call | the agent | the call's outcome | includes `owner <owner>`; an add also `approved <id>` |
