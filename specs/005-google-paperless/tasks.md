---
description: "Task list for feature 005: Google and Paperless, with per-user identity"
---

# Tasks: Google and Paperless, with per-user identity

**Input**: `specs/005-google-paperless/` (plan, spec, research, data-model, contracts, quickstart)

**Tests**: Required (constitution X; FR-018). Unit tests come before the
code in each task; stack tests are written failing before the stack work
they check.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

- [ ] T001 Compose in `deploy/compose.yaml`:
  - `paperless` (`paperlessngx/paperless-ngx:3.5.1`, pinned by digest; database `paperless`, Redis database 2, generated admin credentials, OCR language `eng`, media volume);
  - an internal `docs` network shared only with the gateway;
  - the gateway environment for Paperless and the Google base URLs.

  Plus the `paperless` database in `deploy/postgres/gateway.sql`, and dummy values in `Makefile` for `compose-config`.
- [ ] T002 [P] `deploy/postgres/gateway.sql`: tables `google_connections` and `google_consents` per data-model.md (`Google connection.state` CHECK, `Consent link.secret_sha256`, `Consent link.expires_at`); `sesh_gateway` gets INSERT, SELECT and UPDATE; consents can also be deleted (the PKCE verifier is short-lived)
- [ ] T003 [P] Review `docs/decisions/0008-owners-google-paperless.md`, and the architecture-catalog change already made in `docs/architecture/solutions-architecture.md`

---

## Phase 2: Foundational

- [ ] T004 [P] Unit tests, then in `crates/sesh-core/src/lib.rs`:
  - `Agent.owner`, defaulting to `household` and validated against the users;
  - `User` and `UsersFile` per data-model.md (`User.name`, `User.topic`);
  - `GatewaySettings` gains the Google URLs and limits per data-model.md Limits (`Limits.mail_results_max`, `Limits.upload_max_bytes`, `Limits.upload_types`, `Limits.ref_ttl_secs`).
- [ ] T005 [P] Unit tests, then `crates/sesh-gateway/src/owners.rs`:
  - owner → grant key (data-model.md `Google connection`, the refresh-token key);
  - owner → Paperless owner set (personal: own plus Household; Household: own);
  - owner → approval topic.

  `approvals::gate` and `notify` use the owner's topic (FR-004).
- [ ] T006 [P] Unit tests, then `crates/sesh-gateway/src/refs.rs`: issue and resolve agent-bound attachment refs per data-model.md (`Attachment reference.agent`, `Attachment reference.expires_at`)
- [ ] T007 Unit tests, then `secrets.rs` gains `delete`, and `calls.rs` passes the call's owner to connectors (FR-003)

---

## Phase 3: User Story 1 - Every agent belongs to someone (P1) 🎯 MVP

**Goal**: users exist; agents have owners; approvals route to owners.

**Independent test**: Ana's and Ben's agents get separate approval topics; a Household approval reaches both; a removed owner's agent is refused.

- [ ] T008 [US1] Stack test (written first) `google_docs_stack` in `crates/sesh-cli/tests/stack.rs`:
  - `sesh user add ana`, `sesh user add ben`, and agents with `--owner`;
  - Ana's gated `testgate.act` notifies Ana's topic only;
  - a Household agent's notifies the Household topic, readable as Ana and as Ben;
  - `sesh user list` shows both users;
  - after `sesh user remove ben`, Ben's agent is refused.
- [ ] T009 [US1] CLI:
  - `sesh user add|remove|list` in `crates/sesh-cli/src/user.rs`;
  - `--owner` in `agent.rs`;
  - `ntfy.rs` writes a user and access entries per user;
  - per-user passwords in `secrets.env`.

  With unit tests.

## Phase 4: User Story 2 - Connecting a Google account (P1)

**Goal**: one-time consent puts the owner's grant into the secret store.

**Independent test**: the consent link against fake Google stores the grant; it works once; disconnecting removes it.

- [ ] T010 [US2] Fake Google in `crates/sesh-stub-llm/src/main.rs` per research.md R7: authorization, token, revoke, and per-grant mailboxes. With unit tests.
- [ ] T011 [US2] Unit tests, then the `crates/sesh-gateway/src/google.rs` OAuth part:
  - the consent link (data-model.md `Consent link`);
  - PKCE and the single-use state;
  - the code exchange, and the refresh token into Infisical;
  - the access-token cache;
  - revoke and delete;
  - `invalid_grant` → revoked.

  Pages `/google/start/{id}` and `/google/callback`, and `/admin/google/consent` and `/admin/google/disconnect`, in `pages.rs`.
- [ ] T012 [US2] CLI `sesh google connect|disconnect` in `crates/sesh-cli/src/google.rs`; doctor checks `google` and `paperless` (FR-016) through a gateway admin endpoint
- [ ] T013 [US2] Stack steps in `google_docs_stack`:
  - connect Ana through the link (the fake consent redirects straight back); the link is refused the second time;
  - `sesh user list` shows Ana connected;
  - no refresh or access token appears in outputs, logs, the audit log or the data directory;
  - audit rows exist for the consent request, the connection and the disconnection (FR-015);
  - after disconnecting, calls answer "not connected".

## Phase 5: User Story 3 - Mail and calendar (P1)

**Goal**: mail, calendar and contacts work for the owner; sends and event creation are approved first.

**Independent test**: search and read against fake Gmail; a draft needs no approval; a send waits, then happens once after approval.

- [ ] T014 [US3] Unit tests, then the `connectors/google.rs` descriptors and dispatch per contracts/connectors.md, using `google.rs`. Attachment refs come from `refs.rs`; approval text per contracts/connectors.md.
- [ ] T015 [US3] Fake Gmail, Calendar and People in the stub; `/fake/google/sent` counts sends
- [ ] T016 [US3] Stack steps:
  - Ana's search sees only Ana's mail; Ben's sees only Ben's (FR-018);
  - an extra `user` argument is refused;
  - a draft creates nothing in sent mail;
  - a send gives `approval_pending`, Ana's topic shows the recipients and subject, approving then re-running sends once, and denying sends none;
  - calendar create behaves the same way;
  - a mail body that says "send this to x" still waits for approval.

## Phase 6: User Story 4 - Documents (P2)

**Goal**: documents filed by reference, scoped per owner.

**Independent test**: with the real Paperless, Ana files an attachment by ref; Ben can't see it; a Household upload notifies.

- [ ] T017 [US4] Unit tests (fake Paperless over HTTP), then `connectors/docs.rs`:
  - the admin token;
  - users created as needed;
  - owner-filtered search and get;
  - upload, then polling the task, then setting the owner;
  - type and size limits per data-model.md Limits;
  - notifying on Household uploads.
- [ ] T018 [US4] Stack steps against the real Paperless:
  - Ana files a ref from her mail, and it is found by `docs.search` (within 60 s);
  - Ben can't find or get it;
  - Ben can't use Ana's ref;
  - a disallowed type is refused;
  - a Household agent's upload notifies the Household topic;
  - no attachment bytes appear in any result (SC-004).

## Phase 7: User Story 5 - Evals (P3)

**Goal**: the eval covers mail and documents.

**Independent test**: pass script → the new tasks pass; fail script → `file-attachment` fails for pasted contents.

- [ ] T019 [US5] Fake Paperless in the stub (eval only); eval project: a test user, the fake consent and fake Paperless; suite v3 tasks per research.md R8; stub scripts
- [ ] T020 [US5] `eval_stack` expectations for suite v3 (pass all; fail reasons)

## Phase 8: Polish

- [ ] T021 [P] README, AGENTS.md, quickstart; the Solutions Architecture phase 5 accept line, unchanged unless the build differs
- [ ] T022 Record CI results in `specs/005-google-paperless/research.md`

## Dependencies

- Setup → Foundational → US1 → US2 → US3 → US4 → US5 → Polish.
- T011 needs T005 and T007; T014 needs T006, T010 and T011; T017 needs T006 and T014.

## Implementation Strategy

MVP = US1 + US2: users, owners and a connected Google account. Then mail
(US3), documents (US4), and evals (US5).
