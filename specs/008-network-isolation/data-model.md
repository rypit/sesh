# Data model: Network isolation

There's no stored data. The layout is configuration, held in one table,
`crates/sesh-cli/src/layout.rs`, and mirrored by `deploy/compose.yaml`
and `deploy/compose.stub.yaml`.

## Network

| Field | Notes |
|---|---|
| `name` | Compose network name (`litellm-db`, `models`, `gateway-edge`, …); Docker's is `sesh_<name>` |
| `internal` | `true` for database, pair, resource, cache and stub networks; `false` only for `*-edge`, `feeds-egress` and `searxng-egress` |
| `members` | the services attached; an edge network has exactly one |
| `stub_only` | `true` for `stub`: present only with `compose.stub.yaml` |

**Rules:**
- No network is named `default`.
- An edge network has one member.
- No service is on more than one non-internal network.
- Postgres is only on `*-db` networks.

## Allowed path

| Field | Notes |
|---|---|
| `from` | a service |
| `to` | a service |
| `port` | the destination port |
| `stub_only` | the path exists only in stub mode |
| `kind` | `used` (the stack relies on it) or `reverse` (reachable because the network is two-way, unused) |

Allowed paths follow from shared networks plus the listening port. The
table lists them explicitly, so a shared network that isn't meant as a
path shows up as a test failure.

## Database login rule

| Field | Notes |
|---|---|
| `role` | a service's login (`litellm`, `openwebui`, `sesh_gateway`, `infisical`, `paperless`, `miniflux`) |
| `database` | the one database it may open |
| `network` | its database network (`litellm-db`, …) |
| `subnets` | read from Docker on every `sesh up`, never configured |

**Rules:**
- The generated rules file contains one `host` line per (role,
  database, subnet), plus `local all all trust`, plus a final `host all
  all all reject`.
- There is no rule for the superuser over the network.
- If any subnet can't be read, no rule is written for that role, and
  `sesh up` fails.

## The layout

See [contracts/networks.md](contracts/networks.md).
