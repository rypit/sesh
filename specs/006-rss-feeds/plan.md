# Implementation Plan: RSS feeds

**Branch**: `claude/rss-feeds` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/006-rss-feeds/spec.md`

## Summary

- **Feeds per owner,** in a gateway-owned `feeds` table. The operator
  adds, removes and lists them with `sesh feed …`, through admin
  endpoints, so every change is checked and audited by the gateway.
- **A `feeds` connector:** `list`, `latest`, and `add`, which is
  approval-gated and checked before anyone is asked.
- **One fetching module,** `connectors/feeds.rs`: http/https only, its
  own DNS resolution with every address checked against the refused
  ranges, the checked address pinned for the connection, redirects
  followed by hand (3) and re-checked, 2 MB and 10 s caps, no proxy,
  and a 15-minute cache.
- **Parsing** RSS and Atom with `quick-xml` into plain-text, capped
  items.
- **Tests:** unit tests for every rule, a fake feed server in the stub,
  stack test `feeds_stack`, and eval suite v4.

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002)

**Primary Dependencies**:
- Existing: reqwest (its per-client `resolve` and manual redirects),
  tokio (`lookup_host`), tokio-postgres, axum.
- New crate: `quick-xml` 0.42, MIT, no default features (research.md
  R5).

**Storage**:
- Gateway Postgres: table `feeds` (data-model.md).
- Gateway memory: feed copies and last outcomes.

**Testing**:
- Unit: URL rules, the address filter (every range, v4 and v6, mapped
  forms), redirects to a private address (a local test server), size
  and time caps, RSS/Atom parsing (malformed input, HTML stripping,
  dates), caps, owner scoping, the gate, the cache.
- Stack: `feeds_stack` against the stub's fake feeds, no internet.
- Eval: suite v4 (`feed-headline`, `feed-add`).

**Target Platform**: the existing stack

**Project Type**: a gateway connector, admin endpoints, CLI commands

**Performance Goals**: a cached read answers at once; a fetch within
10 s

**Constraints**:
- No connection to an internal address, ever, outside the test
  allowance.
- No internal address in any error.
- No internet in CI.

**Scale/Scope**: a handful of owners, up to 50 feeds each

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | Fetching, the address filter, scoping and the store are all in the gateway. Feed content is untrusted. |
| II. One way out | Agents reach feeds only through the `feeds` connector. Every feed is operator configuration or a person's approval, and inspectable with `sesh feed list`. |
| III. Agents never see secrets | Feeds need no credentials; none is read. |
| IV. Per-user isolation | Scope comes from the owner (`owners::Scope.readable`); cross-owner negative tests. |
| V. Side effects need approval | `feeds.add` is gated: a new outbound address is a side effect (an exfiltration channel). |
| VI. Agent content never trusted | Feed text is returned as data only; an agent can't write the feed list, only ask; the injected-instruction item is tested in the eval. |
| VII. Local-first | **Exception, justified below**: feeds the household chose are fetched from their publishers, with no household data sent (a plain GET). |
| VIII. Adopt before build | `quick-xml` is adopted for XML; the SSRF guard is a small module because no maintained Rust crate does pinned, re-checked fetching for reqwest. |
| IX. Swappable inference | Unaffected. |
| X. Every guarantee is executable | Unit and stack negative tests for every refused range, redirect, cap and cross-owner read. |

**Component boundary and threat model change:** **ADR 0009** (a new
outbound path to arbitrary public hosts). The architecture document's
catalog, connector table and diagram gain feeds.

Gate: **pass**, with the VII exception below.

## Project Structure

```text
crates/sesh-core/src/lib.rs           # feed settings in GatewaySettings; valid_feed_name
crates/sesh-gateway/src/
├── feeds.rs                          # store: Postgres or memory (one table)
├── connectors/feeds.rs               # seam: URL rules, address filter, guarded fetch, parse, cache, connector
├── connectors/mod.rs                 # + feeds descriptor
├── calls.rs                          # precheck before the gate; owner-scoped dispatch
├── pages.rs                          # /admin/feeds, /admin/feeds/add, /admin/feeds/remove
└── lib.rs, main.rs                   # Gateway.feeds, .feed_cache; allow-hosts only with test connectors
crates/sesh-cli/src/
├── feed.rs                           # sesh feed add|remove|list
├── doctor.rs                         # feeds check
├── up.rs                             # stub mode: feeds_allow_hosts = ["stub-llm"]
└── eval/…                            # suite v4; eval adds its feed
crates/sesh-stub-llm/src/fake_feeds.rs
crates/sesh-cli/tests/stack.rs        # feeds_stack
deploy/postgres/gateway.sql           # + feeds table
.github/workflows/ci.yml              # + feeds_stack
docs/decisions/0009-rss-feeds.md
docs/architecture/{solutions-architecture.md,diagram.jsx}, docs/architecture.svg
```

## Complexity Tracking

| Exception | Why | Simpler alternative rejected because |
|---|---|---|
| VII: fetching feeds from their publishers | Feeds are only useful from their source. A plain GET sends no household data (no cookies, no credentials, a generic user agent); the publisher sees the server's address, as with any reader. | A local mirror would still fetch from the publisher. Naming "feeds the household chose" in VII's exception list is a constitution amendment, left for the operator to decide (ADR 0009); until then this row is the recorded justification. |
