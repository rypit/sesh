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

- [ ] T001 Compose in `deploy/compose.yaml`: `miniflux` (`miniflux/miniflux:2.3.5`, pinned by digest; database `miniflux`; admin `sesh-gateway` with a generated password; no published port) on `feeds`, `feeds-egress` and `feeds-db` per research.md R2; the gateway joins `feeds`, Postgres joins `feeds-db`; `compose.stub.yaml` puts `stub-llm` on `feeds-egress`. Plus the `miniflux` database in `deploy/postgres/gateway.sql`, `MINIFLUX_ADMIN_PASSWORD` in `secrets.rs` `GENERATED`, and a dummy value in `Makefile`
- [ ] T002 [P] `docs/decisions/0009-rss-feeds.md`; constitution VII amended (1.3.0); the architecture document's catalog, connector table, phases (006 feeds, 007 read-only Google Docs, 008 memory, …) and risks; `diagram.jsx`'s feeds box names Miniflux and `docs/architecture.svg` is regenerated

---

## Phase 2: Foundational

- [ ] T003 [P] Unit tests, then `crates/sesh-core/src/lib.rs`: `valid_feed_name`; `upstreams.miniflux`; `feeds_max_per_owner`, validated
- [ ] T004 Unit tests (axum fake Miniflux that returns every user's data), then the Miniflux seam in `crates/sesh-gateway/src/connectors/feeds.rs` per research.md R3: admin sign-in, users by exact name or created, per-process passwords, feeds and entries as the owner's user, every result filtered again by `user_id` and `feed_id`
- [ ] T005 Unit tests, then the URL and name rules and text cleaning per research.md R5–R6

---

## Phase 3: User Story 1 - People choose the feeds (P1) 🎯 MVP

**Goal**: the operator adds, lists and removes feeds; every change is audited.

**Independent test**: add a Household and a personal feed, list, remove; a non-web address refused.

- [ ] T006 [US1] Stack test (written first) `feeds_stack` in `crates/sesh-cli/tests/stack.rs`: users ana and ben; `sesh feed add` for ana, ben and the household against the stub's fake publisher; `ftp://` and `notafeed` refused; `sesh feed list` shows the three; audit rows `feed.added …`
- [ ] T007 [US1] Unit tests, then `/admin/feeds`, `/admin/feeds/add`, `/admin/feeds/remove` in `pages.rs` per contracts/cli.md, with audit rows per data-model.md
- [ ] T008 [US1] Unit tests, then `sesh feed add|remove|list` in `crates/sesh-cli/src/feed.rs` and `main.rs`

## Phase 4: User Story 2 - Agents read their owner's feeds (P1)

**Goal**: `feeds.list` and `feeds.latest`, scoped by owner.

**Independent test**: ana's agent reads ana's and the household's; ben's can't read ana's; an extra `user` argument is refused.

- [ ] T009 [US2] Unit tests, then the `feeds` descriptor and dispatch (`list`, `latest`) in `connectors/feeds.rs`, `connectors/mod.rs` and `calls.rs` (owner in the audit reason) per contracts/connectors.md
- [ ] T010 [US2] The fake publisher in `crates/sesh-stub-llm/src/fake_feeds.rs` per research.md R9, with unit tests
- [ ] T011 [US2] Stack steps: each agent lists only its readable feeds; cross-owner `latest` is "no feed named"; an extra argument is refused; the injected item arrives as data

## Phase 5: User Story 3 - An agent asks to add a feed (P2)

**Goal**: `feeds.add` waits for the owner's approval; bad requests never reach a person.

**Independent test**: ana's agent asks; approval on ana's topic; re-run adds once; a non-web address is refused with no notification.

- [ ] T012 [US3] Unit tests, then `feeds.add`: the precheck before the gate in `calls.rs`; the add after approval in the owner's Miniflux account; nothing added before approval
- [ ] T013 [US3] Stack steps: `feeds.add` gives `approval_pending` on ana's topic; approve, re-run → listed for ana only; `file:///etc/passwd` refused with no new notification; audit row with `approved`

## Phase 6: User Story 4 - The feed service can't reach inside (P1)

**Goal**: Miniflux reaches only the internet and its database; nothing but the gateway reaches it.

**Independent test**: the network probes fail as they should.

- [ ] T014 [US4] Unit tests, then the `feeds` doctor check (`crates/sesh-cli/src/doctor.rs`) per contracts/cli.md; `miniflux` among the services doctor expects
- [ ] T015 [US4] Stack steps: Miniflux is on exactly `feeds`, `feeds-egress` and `feeds-db`; from Miniflux, the gateway and Postgres on `default` are unreachable; an executor can't resolve or reach `miniflux`; Miniflux publishes no port; no Miniflux credential in any result, log, audit row or data-dir file other than `secrets.env`
- [ ] T016 [US4] `feeds_stack` in the CI matrix in `.github/workflows/ci.yml`

## Phase 7: User Story 5 - Evals (P3)

**Goal**: the eval covers feeds.

**Independent test**: pass script → the new tasks pass; fail script → each fails for its reason.

- [ ] T017 [US5] Fake Miniflux in the stub (eval only); eval suite v4 per research.md R9 in `crates/sesh-cli/src/eval/suite.rs`; the eval gateway's `miniflux` upstream is the fakes and `sesh eval` adds the household feed; stub scripts; `feeds` in the eval connectors
- [ ] T018 [US5] `eval_stack` expectations for suite v4

## Phase 8: Polish

- [ ] T019 [P] README status line and roadmap; AGENTS.md commands
- [ ] T020 Record CI results in `specs/006-rss-feeds/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → US5 → Polish.
- T007 needs T004; T009 needs T004 and T005; T012 needs T009.

## Implementation Strategy

MVP = US1 + US2: operator-chosen feeds in Miniflux that each agent
reads within its owner's scope. Then agent adds (US3), the network
checks and doctor (US4), and evals (US5).
