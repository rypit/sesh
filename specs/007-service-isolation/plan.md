# Implementation Plan: Service isolation

**Branch**: `claude/service-isolation` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/007-service-isolation/spec.md`

## Summary

- **A Postgres login per service** (`openwebui`, `litellm`,
  `infisical`, `paperless`, alongside feature 006's `miniflux`), each
  owning only its database, with no administrative rights; CONNECT only
  for the owner. Compose connection strings use them.
- **The migration hands existing installs over in place:** each
  database's owner, then every object in it, by a block that runs in
  each service database (`deploy/postgres/owners.sql`), never `REASSIGN
  OWNED`. It runs on every `sesh up` before any service starts.
- **A Redis per service** (`redis-openwebui`, `redis-infisical`,
  `redis-paperless`), each on an internal network shared only with its
  client, with its own password. The shared `redis` goes.
- **`sesh up` generates missing passwords**, so an upgrade is one
  command.
- **Doctor `db-isolation`**: superuser sessions from other containers,
  database owners, role attributes.
- **ADR 0010**; architecture text; roadmap renumbered (007 isolation,
  008 Google Docs, 009 memory, …).

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002); SQL run by psql

**Primary Dependencies**: no new crates. Two more instances of the
pinned `redis:7.4-alpine` image (three in all, replacing one).

**Storage**: the shared Postgres 16 (pgvector image), unchanged in
layout; ownership changes only. Three Redis volumes replace one.

**Testing**:
- Unit: the migration text and `migration_sql`; the Compose file's
  connection strings; doctor's check; `sesh up` filling passwords and
  the order it starts things.
- Stack: `isolation_stack` (fresh install, pre-007 state, upgrade,
  negative tests), added to the CI matrix.

**Target Platform**: the existing stack

**Project Type**: Compose, SQL migration, CLI

**Performance Goals**: none new; the hand-over runs once per install.

**Constraints**:
- No password in any file but `secrets.env`, on any command line or in
  any log.
- Every existing stack test keeps passing.
- An existing install upgrades with `sesh up` and loses only transient
  Redis state.

**Scale/Scope**: five service logins, three Redis instances

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | Adopted services are untrusted; they no longer hold the superuser, so none can read or change the audit log, the secret store's database or each other's data. |
| II. One way out | Unaffected for agents. The caches are reachable only from their own service. |
| III. Agents never see secrets | Passwords go only to `secrets.env` and, on stdin, to psql; no agent or log sees them. |
| IV. Per-user isolation | Unaffected (per-user scoping stays in the gateway); service isolation is below it. |
| V. Side effects need approval | Unaffected. |
| VI. Agent content never trusted | Unaffected. |
| VII. Local-first | Unaffected; no new outbound traffic. |
| VIII. Adopt before build | Configuration of adopted components only (roles, Compose); each still reached through its one module. |
| IX. Swappable inference | Unaffected. |
| X. Every guarantee is executable | Negative stack tests (cross-database logins, cross-service Redis, doctor on the pre-007 state) and a doctor check. |

**Component boundary and threat-model change:** **ADR 0010** (what
talks to what: a cache per service; what a compromised service can
reach). The architecture diagram doesn't draw storage, so it doesn't
change; the catalog, deployment and phase texts do.

Gate: **pass**.

## Project Structure

```text
deploy/postgres/gateway.sql           # roles, databases, owners, CONNECT for every service
deploy/postgres/owners.sql            # the per-database hand-over block (new)
deploy/postgres/init.sql              # comment only
deploy/compose.yaml                   # service logins; redis-openwebui, -infisical, -paperless and their networks
crates/sesh-cli/src/up.rs             # SERVICE_DATABASES; migration_sql; missing passwords; start order
crates/sesh-cli/src/secrets.rs        # GENERATED: the new passwords; REDIS_PASSWORD dropped
crates/sesh-cli/src/doctor.rs         # db-isolation; the Redis services
crates/sesh-cli/src/host/fake.rs      # per-database SQL output for the fake
crates/sesh-cli/tests/stack.rs        # isolation_stack
crates/sesh-cli/tests/fixtures/pre-007.sql, pre-007.compose.yaml
Makefile                              # compose-config dummies
.github/workflows/ci.yml              # + isolation_stack
docs/decisions/0010-service-isolation.md
docs/architecture/solutions-architecture.md
README.md, AGENTS.md
```

## Complexity Tracking

None.
