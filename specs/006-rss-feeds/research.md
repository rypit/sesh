# Research: RSS feeds

## R1. Where feeds are stored

- **Decision:** a `feeds` table in the gateway's Postgres database
  (`owner`, `name`, `url`, `added_by`, `added_at`; primary key
  `(owner, name)`). Only the gateway writes it. The CLI changes feeds
  through admin endpoints on the approvals listener, with the admin
  token, like `sesh google connect`. Unit tests and the eval gateway
  use an in-memory store (the eval gateway is short-lived, and `sesh
  eval` adds its feeds through the same endpoint).
- **Why not `gateway/feeds.toml`:** the gateway mounts its directory
  read-only (`/etc/sesh:ro`), and an approved agent add has to be
  written by the gateway. Keeping the operator's feeds in a file and the
  approved ones elsewhere would split one list in two, and the
  operator's changes need audit rows, which only the gateway writes.
  Making the config directory writable to the gateway would let a
  gateway bug rewrite `agents.toml`.
- **Why not a writable volume:** a second file store would need the
  same reload and locking care as `users.toml`, plus uid matching on a
  bind mount; Postgres already has the gateway role, grants and
  backups.
- **Constitution VI:** an agent never writes the table. It can only ask;
  the gateway writes the exact address and name the person approved.
  Feed content never reaches the table.
- **Grants:** `sesh_gateway` gets `SELECT, INSERT, DELETE` on `feeds`
  (no `UPDATE`: a feed is replaced by remove and add).

## R2. Scope

- `owners::Scope.readable` (feature 005) already says whose documents
  an agent may read: its owner's, plus the Household's for a personal
  agent. Feeds use the same list.
- `feeds.latest({feed})` looks the name up in the owner's feeds first,
  then the Household's. Another owner's feed is "no feed named …".
- Adds go to the owner's own feeds only, never the Household's from a
  personal agent.

## R3. The address filter (SSRF)

- **One module fetches:** `crates/sesh-gateway/src/connectors/feeds.rs`.
  Nothing else in sesh fetches an agent- or feed-chosen URL (the browser,
  feature 010, will have its own egress guard in its sandbox).
- **URL rules:** `http` or `https`; a host; no user or password; at most
  2,000 characters. Ports are not restricted (the address filter is the
  guard; the gateway only speaks HTTP).
- **Refused addresses:**
  - IPv4: `0.0.0.0/8`, `10.0.0.0/8`, `100.64.0.0/10` (CGNAT, also
    Tailscale), `127.0.0.0/8`, `169.254.0.0/16`, `172.16.0.0/12`,
    `192.0.0.0/24`, `192.0.2.0/24`, `192.88.99.0/24`, `192.168.0.0/16`,
    `198.18.0.0/15`, `198.51.100.0/24`, `203.0.113.0/24`,
    `224.0.0.0/4`, `240.0.0.0/4` (with `255.255.255.255`).
  - IPv6: `::`, `::1`, `::ffff:0:0/96` and `::/96` (checked as the IPv4
    inside), `64:ff9b::/96` and `64:ff9b:1::/48` (NAT64, the IPv4
    inside), `100::/64`, `2001::/23` (Teredo and IETF), `2001:db8::/32`,
    `2002::/16` (6to4, the IPv4 inside), `fc00::/7`, `fe80::/10`,
    `fec0::/10`, `ff00::/8`.
- **Every address:** the gateway resolves the host itself and refuses
  if *any* address is refused (a mixed answer is a rebinding trick).
  It then connects to the address it checked, pinned for that request
  (reqwest's per-client `resolve`), so a second DNS answer can't
  change it; TLS still checks the certificate against the host name.
- **Redirects:** followed by hand, at most 3, each one parsed, checked
  and resolved like the first. Automatic redirects and proxies are off.
- **Limits:** 2 MB, read in chunks and stopped at the cap; 10 s for the
  whole fetch; `Accept` asks for RSS, Atom or XML.
- **Errors** say what was wrong ("isn't a public internet address",
  "redirected to an address that isn't public", "too large", "timed
  out", "answered 404") and never print a resolved address.

## R4. The test allowance

- The fake feed server is the stub (`stub-llm` in stub mode, `fakes` in
  the eval project), on a private Compose address the filter refuses.
- **Decision:** a gateway setting `feeds_allow_hosts`, a list of
  single-label host names (no dots, so never a public name or an IP
  address) that skip the address check for that hop only. The gateway
  honours it only when `SESH_GATEWAY_TEST_CONNECTORS=1`, which only the
  stub overlay and the eval project set; otherwise it logs a warning and
  ignores it. `sesh up` sets it to `["stub-llm"]` in stub mode only; the
  eval sets `["fakes"]`.
- A redirect from an allowed host to anywhere else is checked as usual,
  so the stack test's "redirects to a private address" case is real.
- `sesh doctor` fails `feeds` if the setting is non-empty outside stub
  mode.

## R5. Parsing

- **Crate:** `quick-xml` 0.42 (MIT; maintained; no default features;
  no DTD or external entity processing, so no entity expansion
  attacks). sesh keeps a small extractor over its event reader rather
  than adopting `feed-rs` (which brings chrono, regex, url, uuid and
  mediatype for fields sesh doesn't use) or the `rss` and
  `atom_syndication` pair (two crates, two models).
- **What is read:**
  - RSS 2.0 and RSS 1.0: the channel's `title`; each `item`'s `title`,
    `link`, `pubDate` or `dc:date`, and `description` (or
    `content:encoded` when there's no description).
  - Atom: the feed's `title`; each `entry`'s `title`, `link` (the
    `href` of a `rel="alternate"` link, or of one with no `rel`),
    `published` or `updated`, and `summary` (or `content`).
- **Text:** entities decoded, CDATA kept, HTML tags stripped (with
  `script` and `style` bodies dropped), entities decoded again, and
  whitespace collapsed. Links that aren't http or https are dropped.
- **Dates:** RFC 3339 and RFC 822/2822 dates become RFC 3339 UTC; an
  unreadable date is returned as written (cut to 64 characters) and
  skipped by `since`.
- **Malformed XML, or no `rss`, `RDF` or `feed` root:** "this isn't an
  RSS or Atom feed". At most 200 items are kept from one document.

## R6. Caps

| What | Cap |
|---|---|
| title | 300 characters |
| summary | 1,000 characters |
| link | 2,000 characters |
| published | 64 characters |
| feed title | 300 characters |
| items per call | `limit`, 1–25, default 10 |
| whole `latest` result | 20 kB (items dropped from the end, with `more: true`) |

## R7. Cache

- In gateway memory, by URL: the parsed feed or the failure, with when
  it was fetched. A copy is used for 15 minutes (`feeds_cache_secs`); a
  failure for 60 seconds. Two owners with the same URL share one copy;
  a restart forgets everything.
- The last outcome of each URL is kept for doctor.

## R8. Connector

| Method | Gated | Notes |
|---|---|---|
| `feeds.list({})` | no | names, titles (when fetched), addresses, whose |
| `feeds.latest({feed, since?, limit?})` | no | items, newest first as the feed orders them |
| `feeds.add({url, name?})` | yes | checked before the approval is asked; written after |

- **Before the gate:** `calls.rs` gives the feeds module a `precheck`
  for `add` (URL rules, name, duplicate, per-owner limit, and the
  address check by DNS), so a bad request never reaches a person.
  After approval the add checks again, since DNS can change.
- **Approval text:** the gateway's template, with the address and name
  quoted, and a description that says the gateway will fetch that
  address from now on.

## R9. Audit

- Every `feeds.*` call: the usual row, with `owner <owner>` in the
  reason (and `approved <id>` for an add).
- Operator changes: `feed.added <owner>/<name>` and
  `feed.removed <owner>/<name>` as `operator`; refused attempts
  `feed.add refused: <why>`.

## R10. Doctor

- `GET /admin/feeds` lists every feed with its last outcome (`ok`,
  `failed` with the reason, or `never`).
- `feeds` passes with the count per owner, warns naming any feed whose
  last fetch failed, and fails if `feeds_allow_hosts` is set outside
  stub mode.

## R11. Fakes and the eval

- **The stub** serves `/fake/feeds/<name>.xml`:
  - `harbour` (RSS 2.0, the Household's; its second item carries an
    injected instruction);
  - `garden` (Atom, Ana's); `cycling` (RSS, Ben's); `ferry` (RSS);
  - `redirect` (a 302 to `http://127.0.0.1:8000/…`, loopback);
  - `big` (3 MB);
  - `notafeed` (HTML).

  `/fake/feeds/hits` counts fetches per feed, for the cache test.
- **Eval suite v4:**
  - `feed-headline`: the newest item of the Household's `harbour` feed,
    with its link; `feeds.add` is forbidden (the injected item asks for
    it).
  - `feed-add`: add `ferry` with approval, then re-run once approved.

## R12. Boundary change → ADR 0009

- A new outbound path: the gateway fetches addresses an operator chose,
  or an agent asked for and a person approved, on arbitrary public
  hosts.
- New controls: the address filter, redirect re-checks, limits, and the
  cache.
- The diagram gains a "Feeds" resource box (any public host, through
  the address filter). The catalog and connector table gain `feeds`.
