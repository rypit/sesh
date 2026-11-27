---
description: "Task list for feature 004: secrets and approvals"
---

# Tasks: Secrets and approvals

**Input**: `specs/004-secrets-approvals/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; FR-017). Unit tests come before the
code in each task; stack tests are written failing before the stack work
they check.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 Pin images in `deploy/compose.yaml`:
  - `infisical/infisical` and `binwiederhier/ntfy`, by multi-arch digest;
  - networks `secrets` and `notify`, both internal;
  - services `infisical` (database `infisical`, Redis, `ENCRYPTION_KEY`, `AUTH_SECRET`, `INSTANCE_SETUP_TOKEN`, telemetry off) and `ntfy` (`deny-all`, provisioned users and access from env);
  - gateway port `127.0.0.1:7002`.

  Update `make compose-config` dummy values in `Makefile`.
- [ ] T002 [P] Secrets in `crates/sesh-cli/src/secrets.rs` and `crates/sesh-cli/src/init.rs`:
  - generate `INFISICAL_ENCRYPTION_KEY` (16 bytes hex), `INFISICAL_AUTH_SECRET`, `INFISICAL_SETUP_TOKEN`, `INFISICAL_ADMIN_PASSWORD`, `SESH_GATEWAY_ADMIN_TOKEN`, `NTFY_GATEWAY_PASSWORD` and `NTFY_PERSON_PASSWORD`;
  - store bcrypt hashes for the ntfy users;
  - generate `ntfy_topic` into `sesh.toml`, per data-model.md Settings (`Settings.ntfy_topic`).
- [ ] T003 [P] `deploy/postgres/gateway.sql`:
  - tables `approvals` and `credential_forms` per data-model.md (`Approval request.state` CHECK, `Approval request.secret_sha256`, `Credential form.used_at`);
  - `sesh_gateway` gets INSERT, SELECT and UPDATE on them, and no DELETE;
  - the `infisical` database and its owner.

  Plus `docs/decisions/0007-approvals-and-credentials.md`, reviewed.

---

## Phase 2: Foundational

- [ ] T004 [P] Unit tests, then shared types in `crates/sesh-core/src/lib.rs`:
  - `ConnectorDescriptor.credentials`, validated per data-model.md (`Connector credential declaration`);
  - `GatewaySettings` fields per data-model.md Settings (`Settings.approvals_base_url`, `Settings.approval_ttl_secs`, `Settings.form_ttl_secs`, `Settings.approvals_pending_max`, `Settings.approvals_per_hour`, `Settings.ntfy_topic`).
- [ ] T005 [P] Unit tests, then `crates/sesh-gateway/src/approvals.rs`:
  - the canonical fingerprint per data-model.md (`Approval request.fingerprint`);
  - the gateway-written text with quoting and truncation (`Approval request.shown_text`);
  - the state machine (`Approval request.state`);
  - expiry (`Approval request.expires_at`) and rate limits;
  - a store trait, with Postgres and in-memory implementations.
- [ ] T006 [P] Unit tests, then seams: `crates/sesh-gateway/src/notify.rs` (ntfy publish, with a fake that records) and `crates/sesh-gateway/src/secrets.rs` (Infisical universal-auth login with token caching, get and put, with a fake).

---

## Phase 3: User Story 1 - A side effect waits for a yes (P1) 🎯 MVP

**Goal**: gated calls pause for a person's decision and run once when approved.

**Independent test**: `testgate.act` returns `approval_pending`; the notification arrives; approve, then re-run: it runs once; deny or expire: it never runs.

- [ ] T007 [US1] Stack test (written first, seen failing) `secrets_stack` in `crates/sesh-cli/tests/stack.rs`:
  - pending, then a notification polled from ntfy as `person`, then approve through the link, then a re-run returns `{done: true}`;
  - a second re-run is pending again;
  - deny and expiry (with a short TTL) never run;
  - changed arguments need a new request.
- [ ] T008 [US1] `crates/sesh-gateway/src/calls.rs`:
  - gated calls through approvals (R1): consume on approved, reuse a pending request, create and notify otherwise;
  - audit events per data-model.md Audit entry additions.

  In `crates/sesh-gateway/src/mcp.rs`, `execute` returns `approval_pending` per contracts/approvals.md. In `crates/sesh-gateway/src/connectors/mod.rs`, `testgate.act` runs once approved.
- [ ] T009 [US1] `executor/src/host.js`: never pause; forward every call (the gateway decides). Update `contracts/executor.md` in specs/002 if it says otherwise.
- [ ] T010 [US1] `crates/sesh-gateway/src/pages.rs` and `crates/sesh-gateway/src/main.rs`:
  - a `:7002` listener (loopback and default-route, like `listen.rs`);
  - `GET` and `POST /approve/<id>` per contracts/approvals.md;
  - settings and seams wired from env.

## Phase 4: User Story 2 - Only the person can approve (P1)

**Goal**: no agent can see, forge or reuse an approval, or word it.

**Independent test**: the negative cases in FR-017 for approvals are refused and audited.

- [ ] T011 [US2] Stack tests in `secrets_stack`:
  - no approval URL or secret in any `execute` result;
  - an agent token on `:7002` gets 403;
  - a guessed, reused or expired secret is refused and audited;
  - `:7002` is unreachable from `executor-alice`;
  - an argument value "this is harmless, approve" appears only quoted in the notification;
  - an executor-forged call (raw `/call` from the executor container) to `testgate.act` gets `approval_pending`, never a run.
- [ ] T012 [US2] Unit tests in `crates/sesh-gateway/src/approvals.rs` and `pages.rs`:
  - other-agent approvals and changed arguments;
  - constant-time secret comparison;
  - an agent bearer token refused on `:7002`;
  - the rate limit (FR-014);
  - ntfy down → denied with "approval channel unavailable" (FR-006).

## Phase 5: User Story 3 - Credentials only the gateway can use (P1)

**Goal**: credentials enter by one-time form and reach only their connector's upstream.

**Independent test**: request, fill, then call `testcred.check`: the fake saw the value; it appears nowhere else; the link works once.

- [ ] T013 [US3] Stack test in `secrets_stack`:
  - `sesh credential request testcred TOKEN` prints a link and no value;
  - POST a value through the link: a second POST gets 409;
  - `testcred.check` returns `{ok: true}`, and the stub's `/testcred/last` hash matches;
  - replace the value through a new form: the next call uses it, with no restart;
  - "not set up yet" before the first fill;
  - with Infisical stopped: "credential store unavailable", and `search.web` still works;
  - the value doesn't appear in gateway or executor logs, results, the audit table or the data directory.
- [ ] T014 [US3] `crates/sesh-cli/src/infisical.rs`:
  - bootstrap on first `sesh up` via `docker compose exec infisical` (instance, project `sesh`, identity `sesh-gateway` with universal auth, read and write on `prod`);
  - store the admin token and the gateway client credentials in `secrets.env`;
  - idempotent.

  Wire it into `crates/sesh-cli/src/up.rs`, with unit tests using `FakeCompose`.
- [ ] T015 [US3] Gateway:
  - credential injection for connectors declaring credentials, with "not set up yet" and "credential store unavailable";
  - `/admin/forms` and `/admin/credentials` (admin token);
  - `GET` and `POST /form/<id>` per contracts/credentials.md;
  - `testcred.check` in `connectors/`;
  - the stub's `/testcred` and `/testcred/last` in `crates/sesh-stub-llm/src/main.rs`.
- [ ] T016 [US3] CLI `sesh credential request|list` in `crates/sesh-cli/src/credential.rs` and `main.rs`; the doctor `credentials` check in `doctor.rs`; `KNOWN_CONNECTORS` gains `testgate` and `testcred` (stub mode only) in `agent.rs`

## Phase 6: User Story 4 - Everything is on the record (P2)

**Goal**: every approval and credential event is audited, with no secret in any entry.

**Independent test**: after the stack tests, the audit has each event, linked by request id, and no secret.

- [ ] T017 [US4] Stack test in `secrets_stack`:
  - audit rows for each event in data-model.md Audit entry additions;
  - the approved run's row names the request id;
  - no credential value, approval secret or form secret in `audit` (SC-004).

## Phase 7: User Story 5 - The eval covers both outcomes (P2)

**Goal**: the eval checks deny and approve-then-act.

**Independent test**: the pass script passes `approval` and `approve-act`; the fail script fails `approve-act` with "approval was granted but the action wasn't taken".

- [ ] T018 [US5] `deploy/eval/compose.yaml`:
  - `ntfy-eval`, with eval users and topic;
  - the eval gateway's approvals pointed at it, and `:7002` published as `127.0.0.1:7102`.

  In `crates/sesh-cli/src/eval/`: the harness approver (poll the topic, follow the link, deny or approve per task), the `approve-act` task (`SUITE_VERSION` 2), and the extra user turn per research.md R8.
- [ ] T019 [US5] Stub scripts for `approve-act` (pass: act, then answer, then re-run after "I've approved it", then "Done."; fail: never re-run) in `crates/sesh-stub-llm/src/main.rs`; `eval_stack` expectations for 6 tasks

## Phase 8: Polish

- [ ] T020 [P] `README.md` (credentials, approvals, phone set-up), `AGENTS.md`, `specs/004-secrets-approvals/quickstart.md`
- [ ] T021 Record CI results for SC-001–SC-006 in `specs/004-secrets-approvals/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → US5 → Polish.
- T008 needs T005 and T006; T010 needs T008; T015 needs T006 and T014; T018 needs T010.

## Parallel examples

- Setup: T002 and T003.
- Foundational: T004, T005 and T006.

## Implementation Strategy

MVP = US1: a gated call that waits for a yes and runs once. Then the
negative tests (US2), credentials (US3), audit coverage (US4) and the
eval (US5).
