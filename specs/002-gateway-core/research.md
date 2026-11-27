# Research: Gateway core

Builds on docs/research/codemode.md (Code Mode under local workerd) and
ADRs 0002 and 0004.

## R1. MCP server in Rust → `rmcp` 3.12

Tested 2026-11-26 with a throwaway server:

- `rmcp` 3.12 (official SDK), features `server`,
  `transport-streamable-http-server`, mounted in `axum` 0.8 with
  `StreamableHttpService::new(factory, LocalSessionManager, config)`.
- A manual `ServerHandler` implements `list_tools` and `call_tool`
  (returning `CallToolResponse::Complete(CallToolResult)`).
- **Auth**: an `axum` middleware checks `Authorization: Bearer <token>`
  and refuses unknown tokens with 401 before MCP runs; it inserts the
  agent's identity into the request extensions, which the handler reads
  from `RequestContext.extensions` → `http::request::Parts` →
  `extensions`. Verified: no key → 401; valid key → the handler sees the
  agent.
- `StreamableHttpServerConfig.allowed_hosts` defaults to loopback names;
  the gateway adds `gateway` (its name on the agents' network).
- Build without default features (`chrono` has an advisory, per the
  crate's README).

## R2. Executor → workerd + Code Mode runtime + a small supervisor

- The executor image runs the `@cloudflare/codemode` runtime (pinned
  0.5.6) in workerd (pinned) with a host Worker that accepts
  `POST /execute {code, capability, connectors, deadline_ms}` and builds
  one connector stub per descriptor the gateway sent. Each stub's tool
  `execute` forwards `{capability, connector, method, args}` to the
  gateway's executor-facing endpoint through the Worker's only outbound
  service binding. Scripts run in dynamic workers with
  `globalOutbound: null`.
- Local workerd enforces no CPU limits and runs isolates on one thread
  (codemode.md). A **supervisor** (`sesh-executor-supervisor`, Rust,
  runs inside the executor container, holds nothing secret) starts
  workerd as a child, proxies `/execute` with the request's deadline, and
  on overrun kills and restarts workerd and returns 504. The gateway also
  enforces its own deadline (deadline + 5 s) and doesn't trust the
  supervisor. No component gets the Docker socket.
- One executor container per agent, on its own internal Docker network
  shared only with the gateway, so one agent's runaway script can't
  affect another (FR-007) and executors have no internet.

## R3. Capabilities

Per execution the gateway generates 32 random bytes (URL-safe base64),
keeps `capability → (agent, execution, expires_at)` in memory, sends it
to the executor, and removes it when the execution ends or expires
(deadline + 5 s). Calls on the executor-facing listener must present a
live capability; agent tokens are accepted only on the agent-facing
listener and capabilities only on the executor-facing one. No signing key
is needed because the gateway is the only party that checks them.

## R4. Agents and tokens

`$SESH_DATA_DIR/agents.toml` lists agents (name, SHA-256 of the token,
connector allowlist), mounted read-only into the gateway. `sesh agent
add` generates the token, stores it only in
`$SESH_DATA_DIR/agents/<name>.token` (0600) for the agent's future
container, writes the hash to `agents.toml`, regenerates the per-agent
Compose overlay, and reloads the gateway. Tokens are compared by hash.

## R5. Audit log

Database `gateway`, table `audit` (id, at, agent, execution, connector,
method, args_summary, outcome, reason). Role `sesh_gateway` (password
`GATEWAY_DB_PASSWORD` in `secrets.env`) has `INSERT` only on `audit` and
`USAGE` on its sequence; no `SELECT`, `UPDATE` or `DELETE`. The operator
reads it as the database owner through `docker compose exec postgres
psql`. Argument summaries keep keys and value types and the first 40
characters of strings; nothing from capabilities or tokens. If the insert
fails, the call is refused (FR-010).

Postgres's init script only runs on an empty volume, so `sesh up` runs
an idempotent migration through `docker compose exec postgres psql` as
the owner: create the database, role, table and grants if missing.

## R6. Connectors

| Method | Upstream (configurable base URL) | Notes |
|---|---|---|
| `search.web({query, limit?})` | SearXNG `GET /search?q=…&format=json` | SearXNG container with JSON output enabled, limiter off, on the resources network |
| `search.wiki({query, limit?})` | kiwix-serve `GET /search?pattern=…&format=xml` | parsed from its OpenSearch XML; Kiwix is an optional Compose profile, enabled when ZIM files exist in `$SESH_DATA_DIR/kiwix/` |
| `search.article({path})` | kiwix-serve `GET /content/<path>` | HTML reduced to text |
| `geo.forecast({latitude?, longitude?, days?})` | Open-Meteo `GET /v1/forecast` | defaults to the configured home location |
| `geo.geocode({query})` | Nominatim `GET /search?q=…&format=jsonv2` | generic `User-Agent: sesh` as the usage policy asks; no personal data |
| `geo.nearby({kind, radius_km?})` | Nominatim `GET /search?amenity=…&viewbox=…&bounded=1&format=jsonv2` | around home |

**Verify** at the reference tier: kiwix-serve's search output format and
Nominatim's `amenity` search; tests use fakes that follow these shapes.

All upstream base URLs live in `sesh.toml [gateway.upstreams]`; tests
point them at the fake-upstreams service (the stub server gains routes
for these shapes), so no test reaches the internet (SC-005).

## R7. Search and describe

Implemented in the gateway from its own connector registry: search ranks
`connector.method` names and descriptions by token matches; describe
returns TypeScript declarations generated from each method's JSON
schema (objects, strings, numbers, booleans, arrays, enums). The same
declarations go to the executor so the sandbox's `codemode.describe`
matches.

## R8. Crates

Gateway: `rmcp` (no default features + server, streamable HTTP
server, macros off), `axum`, `tokio`, `reqwest` (rustls), `tokio-postgres`,
`serde`, `serde_json`, `toml`, `sha2`, `getrandom`, `base64`, `anyhow`,
`tracing` (logs without arguments or secrets). Supervisor: std only plus
`tiny_http` and `ureq`. JSON Schema validation is a small in-house
validator for the subset sesh's own connectors declare (types, required,
enums, string length, array items), since the schemas are ours.

## R9. Results (CI, 2026-11-27)

GitHub Actions run 45912985495 on commit 8c39037: `make check` passed; the
stack job built the stub, gateway and executor images and ran both stack
tests in 8 min 46 s. `gateway_stack` (`crates/sesh-cli/tests/stack.rs`)
verified, against the stub upstreams:

- A test MCP client searches, describes, and runs one script calling
  `search.web` and `geo.forecast`; the result's first line marks it as
  untrusted data (US1, FR-001, FR-013).
- An unknown or missing token gets 401; bob neither sees nor reaches `geo`
  (FR-002, FR-003).
- The script finds no environment, `node:fs` or network; the executor
  container reaches only `gateway:7001`, not `gateway:7000`, searxng,
  postgres, litellm or the internet, and holds no secret (FR-004).
- A forged capability, or an agent token in place of one, is refused on
  `/call` (FR-005).
- An approval-gated call returns `approval_unavailable`; an argument
  outside the schema is refused (FR-008, FR-009).
- A `while (true) {}` script times out, bob's script started meanwhile
  completes within 2 s, and alice's next script completes (FR-006, FR-007,
  SC-004).
- The audit log has allowed, approval and schema refusals and token
  refusals; no token appears in it; `sesh_gateway` can't SELECT or DELETE
  (FR-010).
- Removing an agent revokes its token; doctor passes with
  `executors-healthy` (FR-016).

Found by running it: workerd resolves an absolute `embed` path against the
import path (the supervisor passes `--import-path /`), and needs a CA store
to start even with no outbound TLS (the image installs `ca-certificates`).
The executor healthcheck now asks workerd itself, not only the
supervisor's port.

Not yet measured as stated:

- SC-001: the two-call script completes, but its time isn't asserted.
- SC-003: the test checks that each kind of call is audited, not that
  each has exactly one entry (the one-write-per-call rule is unit-tested
  in `calls.rs`).
- SC-005: every upstream is the stub, but CI runners have internet
  access, so "passes with outbound internet blocked" isn't shown.
- SC-006: the audit log is checked for tokens; gateway logs and results
  aren't.
