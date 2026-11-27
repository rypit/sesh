# Implementation Plan: Secrets and approvals

**Branch**: `claude/sharp-ramanujan-6b9nht` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-secrets-approvals/spec.md`

## Summary

**Approvals:** a gated call is decided by the gateway at `/call`, by
fingerprint (research R1).
- A gated call with no approval creates a request stored in Postgres, and
  sends an ntfy notification whose "Review" link opens a gateway approval
  page.
- The agent gets `approval_pending`. After the person approves, the
  identical call runs exactly once.

**Credentials:** they live in Infisical, which `sesh up` bootstraps with
no UI.
- Only the gateway's machine identity can read them.
- They arrive through one-time form links that `sesh credential request`
  creates.
- Connectors declare the credentials they need.

**Tests:**
- Test connectors exercise both paths: `testgate.act` and
  `testcred.check`.
- The eval gains an approve-then-act task, using its own ntfy.
- CI runs the real Infisical and ntfy images, with no internet.

## Technical Context

**Language/Version**: Rust 1.97 (gateway, CLI; ADR 0002)

**Primary Dependencies**:
- Existing: axum, tokio-postgres, reqwest, ureq, sha2, getrandom.
- New: `bcrypt`, used by the CLI for ntfy password hashes.
- New adopted images, both pinned by digest: `infisical/infisical` and
  `binwiederhier/ntfy`.

**Storage**:
- Postgres: new tables `approvals` and `credential_forms` in the
  `gateway` database, and a new `infisical` database.
- Infisical holds credential values.

**Testing**:
- Unit tests, with an in-memory approvals store and fake Infisical and
  ntfy seams.
- Stack tests (`secrets_stack`, plus `eval_stack` additions), run in CI
  with containers' internet blocked.

**Target Platform**: the existing stack (Docker Compose)

**Project Type**: gateway features, CLI commands and Compose services

**Performance Goals**: notification within 2 s of the gated call;
approval page loads in under 1 s

**Constraints**:
- No secret in any output.
- Approval and form endpoints only on loopback and Tailscale.
- Nothing approved by default.

**Scale/Scope**: one approver; tens of approvals a day

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | Decisions, notification text, approval state and credential use are all in the gateway (Rust). The executor is untrusted and only forwards calls. |
| II. One way out | Infisical and ntfy are on networks the agents and executors aren't on. The approvals listener is never on an executor network. |
| III. Agents never see secrets | Credentials are read only by the gateway's Infisical identity, and arrive through one-time forms the person fills. Results, logs and audit hold no value (SC-004). |
| IV. Per-user isolation | Unchanged here. Approvals and credentials are bound to the agent from its token; per-user mapping comes with Phase 5. |
| V. Side effects need approval | The gateway writes the text, delivers it on ntfy (no agent access), binds the approval to the exact call, and runs it once. |
| VI. Agent content never trusted | Argument values appear only quoted and truncated. The executor's approval claims are ignored. |
| VII. Local-first | Infisical and ntfy are self-hosted. The phone reaches them over Tailscale. |
| VIII. Adopt before build | Infisical and ntfy are adopted. Only the approval page and the form are built. |
| IX. Swappable inference | Unaffected. |
| X. Every guarantee is executable | Negative tests for every FR-017 case, the eval's approve and deny tasks, and grep tests for secrets. |

**Approval rules and component boundaries change** (the gateway now
talks to Infisical and ntfy, and serves approval and form pages): **ADR
0007**. The architecture diagram already shows Infisical, ntfy and the
approvals path, so it is unchanged.

Gate: **pass**. Re-checked after design: no violations.

## Project Structure

### Documentation (this feature)

```text
specs/004-secrets-approvals/
├── plan.md, research.md, data-model.md, quickstart.md
├── contracts/approvals.md, contracts/credentials.md
└── tasks.md
```

### Source Code (repository root)

```text
crates/sesh-core/src/lib.rs           # + ConnectorDescriptor.credentials; GatewaySettings approval/form fields
crates/sesh-gateway/src/
├── approvals.rs      # fingerprint, store trait (Postgres + memory), decide/consume, text, rate limit
├── notify.rs         # seam: ntfy publish (+ fake)
├── secrets.rs        # seam: Infisical universal-auth client, get/put (+ fake)
├── pages.rs          # :7002 approval page, form page, /admin/*
├── calls.rs          # gated calls via approvals; credential injection; audit events
├── connectors/test.rs# testgate.act runs; testcred.check
├── mcp.rs            # execute → approval_pending
└── main.rs           # third listener, settings, seams
executor/src/host.js                  # requiresApproval false: the gateway decides
crates/sesh-cli/src/
├── infisical.rs      # bootstrap via compose exec (instance, project, identity)
├── credential.rs     # `sesh credential request|list`
├── up.rs, init.rs, secrets.rs, doctor.rs, main.rs
└── eval/…            # approve-act task; harness approver reading eval ntfy
crates/sesh-stub-llm/src/main.rs      # /testcred fake; approve-act scripts
deploy/compose.yaml                   # + infisical, ntfy, networks secrets/notify; gateway :7002
deploy/postgres/gateway.sql           # + approvals, credential_forms; infisical database
deploy/eval/compose.yaml              # + ntfy-eval
docs/decisions/0007-approvals-and-credentials.md
```

**Structure Decision**: Infisical and ntfy each get one seam module in
the gateway: `secrets.rs` and `notify.rs`. The CLI reaches Infisical only
in `infisical.rs`, and only for bootstrap.

## Complexity Tracking

None.
