# Data model: RSS feeds

The gateway keeps no feed table: Miniflux is the store (research.md R3).

## Feed account (Miniflux user)

| Field | Notes |
|---|---|
| `username` | `sesh-<owner>`; the Household is `sesh-household`; matched exactly |
| `password` | random, set by the gateway the first time it needs the user in a process; memory only |
| `is_admin` | false |

The gateway's own Miniflux account is `sesh-gateway` (admin), with
`MINIFLUX_ADMIN_PASSWORD` from `secrets.env`.

## Feed (Miniflux feed, in the owner's account)

| Field | Notes |
|---|---|
| `title` | the feed's sesh name: `[a-z0-9][a-z0-9-]{0,31}`, unique per owner |
| `feed_url` | http or https, a host, no credentials, ≤ 2,000 characters |
| `user_id` | the owner's Miniflux id; the gateway checks it on every read |
| `parsing_error_count`, `parsing_error_message` | Miniflux's last update error, for doctor |

At most `feeds_max_per_owner` feeds per owner.

## Feed item (what agents get, from a Miniflux entry)

| Field | Notes |
|---|---|
| `title` | plain text, ≤ 300 characters |
| `link` | the entry's `url`, http(s) only, else empty; ≤ 2,000 characters |
| `published` | the entry's `published_at` |
| `summary` | the entry's `content`, markup removed, ≤ 1,000 characters |

## Gateway settings (`GatewaySettings`, `gateway.toml`)

| Setting | Default | Notes |
|---|---|---|
| `upstreams.miniflux` | `http://miniflux:8080` | the eval points it at the fakes |
| `feeds_max_per_owner` | 50 | 1–1,000 |

Environment: `SESH_MINIFLUX_USER` (`sesh-gateway`) and
`SESH_MINIFLUX_PASSWORD`.

## Audit additions

| Event | `agent` | `outcome` | `reason` |
|---|---|---|---|
| Operator adds a feed | `operator` | `allowed` | `feed.added <owner>/<name>` |
| Operator removes a feed | `operator` | `allowed` | `feed.removed <owner>/<name>` |
| Operator add refused | `operator` | `refused` | `feed.add refused: <why>` |
| Every `feeds.*` call | the agent | the call's outcome | includes `owner <owner>`; an add also `approved <id>` |
