---
description: "Task list for feature 007: Service isolation"
---

# Tasks: Service isolation

**Input**: `specs/007-service-isolation/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; FR-011). Unit tests come before the
code in each task; the stack test is written before the stack work it
checks.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [x] T001 [P] `docs/decisions/0010-service-isolation.md`; the architecture document's catalog row for Postgres + Redis, deployment table, backups, phases (007 isolation, 008 read-only Google Docs, 009 memory, …) and risks; README status and roadmap; AGENTS.md

---

## Phase 2: Foundational

- [x] T002 Unit tests, then the new passwords in `GENERATED` (`crates/sesh-cli/src/secrets.rs`), `REDIS_PASSWORD` dropped, and the dummies in `Makefile`

---

## Phase 3: User Story 1 - A compromised service reaches only its own database (P1) 🎯 MVP

**Goal**: a login per service, owning only its database.

**Independent test**: each login opens its own database and no other.

- [x] T003 [US1] Stack test (written first) `isolation_stack` in `crates/sesh-cli/tests/stack.rs`: each login reaches only its own database; none has administrative rights
- [x] T004 [US1] Unit tests, then `deploy/postgres/gateway.sql` per research.md R1–R2: a role per service, its password variable, its database (created if missing) and owner, CONNECT; `sesh_gateway` unchanged
- [x] T005 [US1] Unit tests, then `migration_sql` and `SERVICE_DATABASES` in `crates/sesh-cli/src/up.rs`: every password a `\set` variable, unsafe ones refused
- [x] T006 [US1] Unit tests, then the connection strings in `deploy/compose.yaml`: every service its own login, none the superuser

## Phase 4: User Story 2 - Existing installs keep working after the upgrade (P1)

**Goal**: `sesh up` hands an existing install over in place.

**Independent test**: the pre-007 state, data created, the new `sesh up`, data still there and owned by the roles.

- [x] T007 [US2] Stack steps (written first): the fixtures `tests/fixtures/pre-007.sql` and `pre-007.compose.yaml`; an Open WebUI user, a router key, a credential and a Paperless tag made in the pre-007 state; after `sesh up` all there and writable; no object left with another owner
- [x] T008 [US2] Unit tests, then `deploy/postgres/owners.sql` per research.md R2, appended by `migration_sql` for each service database
- [x] T009 [US2] Unit tests, then `sesh up` generates missing passwords (research.md R5)

## Phase 5: User Story 3 - Each service has its own cache (P1)

**Goal**: a Redis per service, reachable only from it.

**Independent test**: from each service its own Redis is reachable, the others' aren't; the gateway reaches none.

- [x] T010 [US3] Stack steps (written first): `probe_from` each client to each Redis by name and address, its own as the control; the old `redis` is gone after the upgrade
- [x] T011 [US3] Unit tests, then `redis-openwebui`, `redis-infisical`, `redis-paperless` and their networks in `deploy/compose.yaml`; `sesh up` starts them; doctor expects them

## Phase 6: User Story 4 - Doctor shows the isolation holds (P2)

**Goal**: `db-isolation`.

**Independent test**: fails on the pre-007 state; passes after the upgrade.

- [x] T012 [US4] Unit tests, then `db-isolation` in `crates/sesh-cli/src/doctor.rs` per research.md R6 and contracts/cli.md (the fake compose answers per database)
- [x] T013 [US4] Stack steps: doctor fails in the pre-007 state, naming the owners, and passes after `sesh up`
- [x] T014 [US4] `isolation_stack` in the CI matrix in `.github/workflows/ci.yml`

## Phase 7: Polish

- [ ] T015 Record CI results (every stack test, including the existing ones) in `specs/007-service-isolation/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → Polish.
- T008 needs T004; T013 needs T012.

## Implementation Strategy

MVP = US1 + US2: every service under its own login, existing installs
handed over in place. Then the caches (US3) and doctor (US4).
