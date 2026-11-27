# Implementation Plan: Google and Paperless, with per-user identity

**Branch**: `claude/sharp-ramanujan-6b9nht` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-google-paperless/spec.md`

## Summary

- **Owners:** agents gain an owner, a user or the Household, and the
  gateway derives every per-user scope from it at call time.
- **Approval channels:** each user gets an ntfy topic. The Household
  topic is read by everyone.
- **Google:** the gateway calls Google's APIs itself. A consent link
  puts the owner's refresh token into Infisical, using PKCE and a
  single-use state. Connectors `mail`, `calendar` and `contacts` work
  off that grant; `mail.send` and `calendar.create` are gated.
- **Paperless-ngx** joins the stack on an internal `docs` network. The
  gateway's `docs` connector is the one module that reaches it: as
  admin, filtering by owner and setting the owner on upload.
- **Filing:** attachments are filed by an agent-bound reference, so
  their bytes stay in the gateway.
- **Fakes and evals:** the stub fakes Google, plus Paperless for the
  eval. The eval gains mail and documents tasks.

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002)

**Primary Dependencies**:
- Existing: reqwest (with the `multipart` feature added), axum,
  tokio-postgres, sha2, base64.
- New adopted image: `paperlessngx/paperless-ngx:3.5.1`, pinned by
  digest.

**Storage**:
- `gateway/users.toml`.
- New gateway Postgres tables: `google_connections` and
  `google_consents`.
- Infisical: refresh tokens, and the Google client id and secret.
- Paperless: its own database, plus a media volume.

**Testing**:
- Unit tests with fake Google, Paperless, Infisical and ntfy seams.
- Stack test `google_docs_stack`, run against fake Google (the stub) and
  the real Paperless.
- `eval_stack` grows to suite v3.

**Target Platform**: the existing stack

**Project Type**: gateway connectors, CLI commands and a Compose
service

**Performance Goals**: documents are searchable within 1 minute of
filing (SC-005); the consent flow takes under 2 minutes (SC-001)

**Constraints**:
- No grant, token or file bytes in any output.
- Cross-user access is refused.
- No internet in CI.

**Scale/Scope**: a handful of users; mailboxes of any size, with
searches capped

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | OAuth, token handling, owner scoping and byte transfer all happen in the gateway. |
| II. One way out | Agents reach Google and Paperless only through connectors. Paperless sits on an internal network shared only with the gateway. |
| III. Agents never see secrets | Refresh and access tokens, the client secret and the Paperless admin credential never leave the gateway. The grant enters through the person's own consent, never typed or shown. |
| IV. Per-user isolation | Scope comes from the owner, never from the model. Cross-user negative tests for mail, calendar, contacts and documents. |
| V. Side effects need approval | `mail.send` and `calendar.create` are gated, with gateway-written text, routed to the owner (the Household to every user). |
| VI. Agent content never trusted | Mail and document content is labelled untrusted. An email telling the agent to send still waits for approval. |
| VII. Local-first | Google is a named exception (OAuth). Paperless is local. |
| VIII. Adopt before build | Paperless-ngx is adopted. Google is used through its own APIs; a Google MCP server was dropped (ADR 0008). |
| IX. Swappable inference | Unaffected. |
| X. Every guarantee is executable | FR-018's negative tests, the eval tasks, and grep tests for tokens and bytes. |

**Component boundary and scoping rules change:** **ADR 0008**. The
architecture's catalog is updated (Google is called by the gateway, not
through an MCP server); the diagram's boxes are unchanged.

Gate: **pass**.

## Project Structure

```text
crates/sesh-core/src/lib.rs        # Agent.owner; User; UsersFile; settings for Google URLs and limits
crates/sesh-gateway/src/
├── owners.rs                      # owner → scopes (grant key, Paperless ids, topic)
├── google.rs                      # seam: OAuth (consent, token, revoke) and Gmail/Calendar/People calls
├── connectors/google.rs           # mail, calendar, contacts descriptors and dispatch
├── connectors/docs.rs             # seam: Paperless (admin token, users, search, get, upload, owner)
├── refs.rs                        # attachment references
├── pages.rs                       # + /google/start, /google/callback, /admin/google/*
├── approvals.rs, notify.rs        # notify the owner's topic
└── calls.rs                       # owner-scoped dispatch
crates/sesh-cli/src/
├── user.rs                        # sesh user add/remove/list; ntfy users and topics
├── google.rs                      # sesh google connect/disconnect
├── agent.rs                       # --owner
└── doctor.rs, up.rs, ntfy.rs, eval/…
crates/sesh-stub-llm/src/main.rs   # fake Google; fake Paperless (eval); scripts for suite v3
deploy/compose.yaml                # + paperless, docs network
deploy/postgres/gateway.sql        # + google tables; paperless database
docs/decisions/0008-owners-google-paperless.md
```

## Complexity Tracking

None.
