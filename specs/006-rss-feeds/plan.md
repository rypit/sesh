# Implementation Plan: RSS feeds

**Branch**: `claude/rss-feeds` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/006-rss-feeds/spec.md`

## Summary

- **Miniflux is adopted** as the feed service: it fetches, parses and
  stores feeds, on its own networks (`feeds` with the gateway,
  `feeds-egress` to publishers, `feeds-db` to Postgres), never on
  `default`.
- **One module,** `connectors/feeds.rs`, is the gateway's only route to
  Miniflux. It manages one Miniflux user per owner as admin, reads and
  adds feeds as that user, and filters every result again by owner.
- **A `feeds` connector:** `list`, `latest`, and `add`, which is
  approval-gated and checked before anyone is asked.
- **`sesh feed add|remove|list`** through gateway admin endpoints, so
  every change is audited by the gateway.
- **The gateway never fetches a feed address** (research.md R0).
- **Tests:** unit tests against a fake Miniflux that ignores scope, the
  real Miniflux in `feeds_stack` with the stub as publisher, a fake
  Miniflux in the stub for eval suite v4.

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002)

**Primary Dependencies**:
- Existing: reqwest, axum, serde_json.
- New adopted image: `miniflux/miniflux:2.3.5`, pinned by digest.

**Storage**:
- Miniflux: its own Postgres database `miniflux` (feeds, entries, users).
- Gateway memory: each owner's Miniflux user id and session password.
- No new gateway table.

**Testing**:
- Unit: URL and name rules, owner scoping with a fake Miniflux that
  returns every user's data, user creation by exact name, the gate,
  text cleaning and caps, the admin endpoints, doctor, the CLI.
- Stack: `feeds_stack` with the real Miniflux; network negative tests.
- Eval: suite v4 (`feed-headline`, `feed-add`) against fake Miniflux.

**Target Platform**: the existing stack

**Project Type**: a Compose service, a gateway connector, admin
endpoints, CLI commands

**Performance Goals**: a feed added by the operator is readable within
1 minute (SC-005)

**Constraints**:
- Miniflux reaches only the internet and its own database.
- No Miniflux credential in any output.
- No internet in CI.

**Scale/Scope**: a handful of owners, up to 50 feeds each

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | Scoping, approvals and audit are in the gateway. Miniflux is untrusted; its results are filtered again by owner. Feed content is untrusted. |
| II. One way out | Agents reach feeds only through the `feeds` connector. Miniflux sits on an internal network shared only with the gateway. Every feed is operator configuration or a person's approval, and inspectable with `sesh feed list`. |
| III. Agents never see secrets | The Miniflux admin password and per-owner passwords stay in the gateway. |
| IV. Per-user isolation | One Miniflux user per owner; scope from the owner; cross-owner negative tests. |
| V. Side effects need approval | `feeds.add` is gated: a new address fetched for the owner is a side effect. |
| VI. Agent content never trusted | Feed text is returned as data only; an agent can only ask to add a feed; the injected-instruction item is tested in the stack test and the eval. |
| VII. Local-first | Feed publishers are a named exception, reached only through the feed service (constitution 1.3.0). |
| VIII. Adopt before build | Miniflux is adopted for fetching, parsing and storage, reached through one module. |
| IX. Swappable inference | Unaffected. |
| X. Every guarantee is executable | Unit tests for owner filtering against a fake that ignores scope; stack tests for isolation, approval and network reach. |

**Component boundary change:** **ADR 0009** (a new adopted component and
three networks). The architecture document's catalog, connector table
and diagram gain feeds (Miniflux).

Gate: **pass**.

## Project Structure

```text
crates/sesh-core/src/lib.rs           # upstreams.miniflux; feeds_max_per_owner; valid_feed_name
crates/sesh-gateway/src/
├── connectors/feeds.rs               # seam: Miniflux (admin, users per owner, feeds, entries), the connector
├── connectors/mod.rs                 # + feeds descriptor
├── calls.rs                          # precheck before the gate; owner-scoped dispatch
├── pages.rs                          # /admin/feeds, /admin/feeds/add, /admin/feeds/remove
└── lib.rs, main.rs                   # Gateway.miniflux
crates/sesh-cli/src/
├── feed.rs                           # sesh feed add|remove|list
├── doctor.rs                         # feeds check; miniflux among the services
├── secrets.rs                        # MINIFLUX_ADMIN_PASSWORD
└── eval/…                            # suite v4; the eval adds its feed
crates/sesh-stub-llm/src/fake_feeds.rs     # the fake publisher
crates/sesh-stub-llm/src/fake_miniflux.rs  # the eval's fake Miniflux
crates/sesh-cli/tests/stack.rs        # feeds_stack
deploy/compose.yaml                   # + miniflux; networks feeds, feeds-egress, feeds-db
deploy/compose.stub.yaml              # stub-llm on feeds-egress
deploy/postgres/gateway.sql           # + miniflux database
.github/workflows/ci.yml              # + feeds_stack
.specify/memory/constitution.md       # VII amended, 1.3.0
docs/decisions/0009-rss-feeds.md
docs/architecture/{solutions-architecture.md,diagram.jsx}, docs/architecture.svg
```

## Complexity Tracking

None. (A first draft recorded a VII exception for the gateway fetching
feeds itself; the gateway no longer fetches, and VII names feed
publishers behind the feed service.)
