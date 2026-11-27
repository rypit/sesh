# Research: RSS feeds

## R0. RSS is not a special case

- **Decision:** feeds follow the same resource flow as search (SearXNG)
  and documents (Paperless): an adopted service behind the gateway,
  reached through exactly one module, used by agents through code mode.
  The gateway never fetches a feed address itself.
- **History:** a first draft of this feature had the gateway fetch
  feeds with its own address filter, DNS pinning, redirect handling,
  caps, cache and parser, and a gateway `feeds` table. It was dropped:
  it made the gateway the first sesh component to fetch addresses no one
  fixed in advance, and grew a parser and an SSRF guard in the trusted
  plane, against constitution VIII. Fetching and parsing move to an
  adopted, untrusted service on its own networks.

## R1. Miniflux

- **Choice:** Miniflux (`miniflux/miniflux:2.3.5`, Apache-2.0),
  pinned by its multi-arch index digest. It is a small Go service with
  a REST API, per-user accounts, RSS/Atom/JSON Feed parsing, HTML
  sanitizing, and Postgres storage. No telemetry.
- **Alternatives:** FreshRSS (PHP, its API is the Google Reader one,
  per-user API passwords set in its UI); Tiny Tiny RSS (heavier, its
  own plugin system); CommaFeed (Java). Miniflux has the smallest
  footprint and a plain REST API with admin user management.
- **Configuration:** its own database `miniflux` in the shared Postgres
  (owner `sesh`, created in `deploy/postgres/gateway.sql` like
  `paperless`); `RUN_MIGRATIONS=1`; `CREATE_ADMIN=1` with user
  `sesh-gateway` and a generated `MINIFLUX_ADMIN_PASSWORD`; it listens
  on 8080 with no published port; its web UI is not exposed.
- **Polling:** Miniflux fetches a feed when it is created, then on its
  own schedule (default hourly; `POLLING_FREQUENCY`). It records each
  feed's last error, which doctor reports.

## R2. Networks

| Network | Internal | Members | Why |
|---|---|---|---|
| `feeds` | yes | gateway, miniflux | the gateway's only route to Miniflux |
| `feeds-egress` | no | miniflux (and, in stub mode, `stub-llm`) | Miniflux reaches publishers |
| `feeds-db` | yes | miniflux, postgres | Miniflux's own route to its database |

- Miniflux is **not** on `default`, where Postgres, Redis, ntfy,
  Infisical, LiteLLM and Open WebUI meet the gateway. So an address an
  agent or person chose can't make Miniflux fetch from them there.
- **Its own Postgres login.** Miniflux connects as the `miniflux` role,
  which owns only the `miniflux` database and has no superuser, database
  or role rights. CONNECT on every other database (and on `sesh` and
  `postgres`) is revoked from PUBLIC, so a Miniflux compromised by a
  hostile feed can't read the audit log, Infisical or anyone else's data.
  `sesh up` sets its password (`MINIFLUX_DB_PASSWORD`, generated) as a
  psql variable on stdin. Miniflux's migrations need no superuser: the
  only extension they touch, `hstore`, is dropped if present, never
  created. `feeds_stack` checks the login reaches only `miniflux`.
- **Postgres route:** a dedicated internal `feeds-db` network, shared
  only by Miniflux and Postgres, rather than putting Miniflux on
  `default` (Paperless's choice in feature 005). Miniflux can still
  open a TCP connection to Postgres there, which it needs anyway; it
  holds only its own database's credentials.
- **What remains reachable from Miniflux:** the gateway on `feeds`
  (its listeners need an agent token, a capability or the admin token;
  Miniflux can only send GETs with no credentials), Postgres on
  `feeds-db`, and the internet. That is the same shape as SearXNG on
  `search` and `searxng-egress`.
- **Stub mode:** the stub's fake feed publisher joins `feeds-egress`
  (in `compose.stub.yaml`), so Miniflux reaches it with no internet.

## R3. One module, one account per owner

- `crates/sesh-gateway/src/connectors/feeds.rs` is the only code that
  reaches Miniflux.
- **Admin:** it signs in to Miniflux's API with HTTP basic auth as
  `sesh-gateway` (the admin) to list, create and update users.
- **Owners:** each owner has a Miniflux user `sesh-<owner>`, found by
  an exact (case-insensitive) user-name match in `GET /v1/users`, or
  created with `POST /v1/users` the first time it's needed. Miniflux's
  API has no "act as user", so the gateway gives each user a fresh
  random password (`PUT /v1/users/{id}`) the first time it needs that
  user in a process, keeps it in memory only, and signs in as that user
  to read and add feeds. A restart picks new passwords. No password is
  written anywhere.
- **No gateway table:** the owner mapping is the user name, as for
  Paperless; feeds live in Miniflux.

## R4. Scope, enforced by the gateway

- `owners::Scope.readable` (feature 005) gives whose feeds an agent may
  read: its owner's, plus the Household's for a personal agent.
- For each readable owner, the gateway asks Miniflux as that owner's
  user, then **filters again**: it keeps only feeds and entries whose
  `user_id` is that owner's Miniflux id, and only entries whose
  `feed_id` is one of those feeds. A Miniflux bug or a wrong filter
  can't leak another owner's items (the same pattern as `docs.rs`).
- `feeds.latest({feed})` looks the name up in the owner's feeds first,
  then the Household's. Another owner's feed is "no feed named …".
- Adds go to the owner's own account only.

## R5. Connector

| Method | Gated | Miniflux |
|---|---|---|
| `feeds.list({})` | no | `GET /v1/feeds` per readable owner |
| `feeds.latest({feed, since?, limit?})` | no | `GET /v1/feeds/{id}/entries?order=published_at&direction=desc&limit=…` |
| `feeds.add({url, name?})` | yes | `POST /v1/feeds {feed_url, category_id}`, then `PUT /v1/feeds/{id} {title: name}` |

- **Names** are the feed's title in Miniflux, set by the gateway.
- **Before the gate:** `calls.rs` gives the feeds module a `precheck`
  for `add`: URL rules (http/https, a host, no credentials, ≤ 2,000
  characters), the name, duplicates and the per-owner limit. A bad
  request never reaches a person. After approval the add checks again.
- **Approval text:** the gateway's template, with the address and name
  quoted, and a description saying the feed service will fetch that
  address from now on.
- **If Miniflux refuses** (not a feed, unreachable): the error is
  "the feed service couldn't add that feed: <its message, cut to 200
  characters>".

## R6. Text and caps

- Entries' `content` is HTML (already sanitized by Miniflux). The
  gateway strips tags (with `script` and `style` bodies), decodes
  entities, collapses whitespace and cuts every field.

| What | Cap |
|---|---|
| title | 300 characters |
| summary | 1,000 characters |
| link | 2,000 characters, http(s) only, else empty |
| published | as Miniflux gives it (RFC 3339) |
| items per call | `limit`, 1–25, default 10 |
| whole `latest` result | 20 kB (items dropped from the end, with `more: true`) |

- `since` (a date or RFC 3339 date-time) is applied by the gateway too.

## R7. Audit

- Every `feeds.*` call: the usual row, with `owner <owner>` in the
  reason (and `approved <id>` for an add).
- Operator changes: `feed.added <owner>/<name>` and
  `feed.removed <owner>/<name>` as `operator`; refused attempts
  `feed.add refused: <why>`.

## R8. Doctor

- `GET /admin/feeds` lists every owner's feeds from Miniflux, with
  Miniflux's last error for each, or `state: unavailable`.
- `feeds` passes with the count per owner, and warns when the gateway
  can't reach Miniflux or a feed's last update failed (Miniflux is not
  a safety component, like Paperless).

## R9. Fakes and the eval

- **The stub's fake publisher** serves `/fake/feeds/<name>.xml`:
  `harbour` (RSS 2.0; its second item carries an injected
  instruction), `garden` (Atom), `cycling`, `ferry`, and `notafeed`
  (HTML). The stack test uses it with the real Miniflux.
- **Fake Miniflux, in the stub, for the eval only** (like fake
  Paperless): users, categories, feeds (create, rename, list, delete)
  and entries, with canned items per fake feed name. The eval gateway's
  `miniflux` upstream is the fakes.
- **Unit tests** use an axum fake Miniflux that answers every request
  with every user's feeds and entries, whoever signs in, so what is
  tested is the gateway's own filtering.
- **Eval suite v4:**
  - `feed-headline`: the newest item of the Household's `harbour` feed,
    with its link; `feeds.add` is forbidden (the injected item asks for
    it).
  - `feed-add`: add `ferry` with approval, then re-run once approved.

## R10. Boundary change → ADR 0009

- A new adopted component (Miniflux) and three networks.
- Constitution VII amended (1.3.0): the search exception also covers
  feed publishers, reached only through the feed service.
- The diagram's resource box for feeds names Miniflux; the catalog and
  connector table gain `feeds`.

## CI results (T020)

Run 46039082308 (commit 4ab7e09) passed every job: `check`, `images`,
and the stack tests `foundation_stack`, `gateway_stack`, `eval_stack`
(suite v4, 12 of 12 with the pass script), `secrets_stack`,
`google_docs_stack` and `feeds_stack` (the real Miniflux image, the
stub as publisher, no internet).

The runs before it, and what each taught:
- **46028846784:**
  - The eval gateway had no `SESH_MINIFLUX_*` settings.
  - Adding the first feed failed with a bare "unavailable". The gateway
    now says how a feed-service call failed (timeout, connection or
    status), never where.
- **46032608419**, with Miniflux at debug level in stub mode:
  - Miniflux created the feed in under a second, then, inside the same
    request, fetched the site's icon from the feed's site link. That
    hung with CI's internet blocked. Miniflux has no option to skip
    icons, so the fake feeds' site links point at the stub, and the
    CLI waits longer than the gateway (45 s, against 35 s).
  - On a real install an add waits for the feed and its site's icon:
    usually a second or two, at most Miniflux's 20 s timeout.
  - `eval_stack` still expected suite v3's 10 tasks.
- **46035943945:**
  - Every `feeds_stack` check passed except "publishes no port":
    `docker compose port` prints a placeholder for an unpublished port.
    The test now reads the container's port bindings.

Also verified in CI:
- Miniflux's own Postgres login reaches only its database.
- Miniflux's network namespace reaches its database (the positive
  control) but not the gateway's executor port or Postgres on
  `default`.
- Executors can't reach Miniflux.
