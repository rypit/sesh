---
description: "Task list for feature 006: RSS feeds"
---

# Tasks: RSS feeds

**Input**: `specs/006-rss-feeds/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; FR-014). Unit tests come before the
code in each task; the stack test is written before the stack work it
checks.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 `deploy/postgres/gateway.sql`: table `feeds` per data-model.md (`Feed.owner`, `Feed.name`, primary key), with `SELECT, INSERT, DELETE` for `sesh_gateway`
- [ ] T002 [P] `docs/decisions/0009-rss-feeds.md`; the architecture document's catalog, connector table, phases (006 feeds, 007 read-only Google Docs, 008 memory, …) and risks; `diagram.jsx` gains a Feeds box and `docs/architecture.svg` is regenerated
- [ ] T003 [P] `quick-xml` in `crates/sesh-gateway/Cargo.toml` (no default features); `cargo deny check` passes

---

## Phase 2: Foundational

- [ ] T004 [P] Unit tests, then `crates/sesh-core/src/lib.rs`: `valid_feed_name`; feed settings per data-model.md Gateway settings (`feeds_cache_secs`, `feeds_max_bytes`, `feeds_timeout_secs`, `feeds_max_per_owner`, `feeds_allow_hosts`), validated (single-label allow hosts)
- [ ] T005 [P] Unit tests, then `crates/sesh-gateway/src/feeds.rs`: the store (memory and Postgres) per data-model.md Feed: add (duplicate and per-owner limit refused), remove, list by owner and by readable owners
- [ ] T006 Unit tests, then the address filter and URL rules in `crates/sesh-gateway/src/connectors/feeds.rs` per research.md R3: every refused range, v4 and v6, mapped and NAT64 forms; http/https only; no credentials; length cap; default names
- [ ] T007 Unit tests (a local test server), then the guarded fetch: own resolution, pinned address, manual redirects re-checked (a redirect to loopback refused), 3-redirect cap, size cap, timeout, no proxy; allow hosts honoured only with test connectors (`lib.rs` helper, used by `main.rs`)
- [ ] T008 Unit tests, then parsing per research.md R5–R6: RSS 2.0, RSS 1.0, Atom; malformed and non-feed input; HTML stripping; dates; caps; total result cap
- [ ] T009 Unit tests, then the cache per research.md R7 (one fetch per 15 minutes; failures for 60 s) and last outcomes

---

## Phase 3: User Story 1 - People choose the feeds (P1) 🎯 MVP

**Goal**: the operator adds, lists and removes feeds; every change is audited.

**Independent test**: add a Household and a personal feed, list, remove; internal and non-web addresses refused.

- [ ] T010 [US1] Stack test (written first) `feeds_stack` in `crates/sesh-cli/tests/stack.rs`: users ana and ben; `sesh feed add` for ana, ben and the household; `ftp://` and `http://127.0.0.1/` refused; `sesh feed list` shows the three; audit rows `feed.added …`
- [ ] T011 [US1] Unit tests, then `/admin/feeds`, `/admin/feeds/add`, `/admin/feeds/remove` in `pages.rs` per contracts/cli.md, with audit rows per data-model.md
- [ ] T012 [US1] Unit tests, then `sesh feed add|remove|list` in `crates/sesh-cli/src/feed.rs` and `main.rs`; stub mode sets `feeds_allow_hosts = ["stub-llm"]` in `up.rs`

## Phase 4: User Story 2 - Agents read their owner's feeds (P1)

**Goal**: `feeds.list` and `feeds.latest`, scoped by owner.

**Independent test**: ana's agent reads ana's and the household's; ben's can't read ana's; an extra `user` argument is refused.

- [ ] T013 [US2] Unit tests, then the `feeds` descriptor and dispatch (`list`, `latest`) in `connectors/feeds.rs`, `connectors/mod.rs` and `calls.rs` (owner in the audit reason) per contracts/connectors.md
- [ ] T014 [US2] Fake feeds in `crates/sesh-stub-llm/src/fake_feeds.rs` per research.md R11, with unit tests
- [ ] T015 [US2] Stack steps: each agent lists only its readable feeds; cross-owner `latest` is "no feed named"; extra argument refused; the injected item arrives as data; the cache: two reads, one fetch (`/fake/feeds/hits`)

## Phase 5: User Story 3 - An agent asks to add a feed (P2)

**Goal**: `feeds.add` waits for the owner's approval; bad requests never reach a person.

**Independent test**: ana's agent asks; approval on ana's topic; re-run adds once; an internal address is refused with no notification.

- [ ] T016 [US3] Unit tests, then `feeds.add`: the precheck before the gate in `calls.rs`; the add after approval (checked again); `approval_pending` first; nothing added before approval
- [ ] T017 [US3] Stack steps: `feeds.add` gives `approval_pending` on ana's topic; approve, re-run → listed for ana only; `http://169.254.169.254/` and `http://postgres:5432/` refused with no new notification; audit row with `approved`

## Phase 6: User Story 4 - Feeds that misbehave can't reach inside (P1)

**Goal**: redirects, size and format failures are refused without leaking.

**Independent test**: the redirecting and oversized fakes are refused; no internal address in any result.

- [ ] T018 [US4] Unit tests, then the `feeds` doctor check (`crates/sesh-cli/src/doctor.rs`) per contracts/cli.md
- [ ] T019 [US4] Stack steps: `redirect`, `big` and `notafeed` refused with their reasons; doctor warns naming them, then passes once they're removed; no private address (10/8, 172.16/12, 192.168/16) in any result or audit row
- [ ] T020 [US4] `feeds_stack` in the CI matrix in `.github/workflows/ci.yml`

## Phase 7: User Story 5 - Evals (P3)

**Goal**: the eval covers feeds.

**Independent test**: pass script → the new tasks pass; fail script → each fails for its reason.

- [ ] T021 [US5] Eval suite v4 per research.md R11 in `crates/sesh-cli/src/eval/suite.rs`; the eval gateway's `feeds_allow_hosts = ["fakes"]` and its household feed; stub scripts; `feeds` in the eval connectors
- [ ] T022 [US5] `eval_stack` expectations for suite v4

## Phase 8: Polish

- [ ] T023 [P] README status line and roadmap; AGENTS.md commands
- [ ] T024 Record CI results in `specs/006-rss-feeds/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → US5 → Polish.
- T007 needs T006; T009 needs T007 and T008; T011 needs T005; T013 needs T009; T016 needs T013.

## Implementation Strategy

MVP = US1 + US2: operator-chosen feeds that each agent reads within its
owner's scope, behind the address filter. Then agent adds (US3), the
misbehaving-feed checks and doctor (US4), and evals (US5).
