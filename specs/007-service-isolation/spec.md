# Feature Specification: Service isolation

**Feature Branch**: `claude/service-isolation`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Feature 007 — service isolation. Every adopted service except the gateway and Miniflux logs in to the shared Postgres as the superuser `sesh`: Open WebUI, LiteLLM, Infisical and Paperless. Any one of them compromised could read or change every database, including the gateway's audit log and Infisical's. Redis is shared by Open WebUI, Infisical and Paperless with one password, so each can read and write the others' data. Give each service its own Postgres login that owns only its own database, as was done for Miniflux, with existing installs migrated in place; isolate Redis per service; add a doctor check; record the threat-model change in an ADR. Read-only Google Docs moves to 008, memory to 009, and so on. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **The operator** runs sesh and upgrades it.
- **An attacker** has taken over one adopted service (Open WebUI,
  LiteLLM, Infisical, Paperless or Miniflux), for example through a
  bug reached by a chat message, a document or a feed.

### User Story 1 - A compromised service reaches only its own database (Priority: P1)

Each adopted service that stores data in Postgres logs in with its own
login. That login owns the service's database and nothing else: it
can't open another service's database, the gateway's audit log, or
the administrative databases, and it can't create databases or roles.

**Why this priority**: Constitution I. The audit log and the secret
store's database are control-plane data; today any adopted service can
read or rewrite them.

**Independent Test**: In the running stack, each service's login opens
its own database and is refused by every other one; none of the logins
is a superuser or may create databases or roles.

**Acceptance Scenarios**:

1. **Given** the running stack, **When** a service's login connects to
   its own database, **Then** it succeeds.
2. **Given** the running stack, **When** a service's login connects to
   any other database (another service's, `gateway`, `sesh`,
   `postgres`), **Then** it is refused.
3. **Given** a service's login, **When** its attributes are read,
   **Then** it has no superuser, create-database or create-role right.

---

### User Story 2 - Existing installs keep working after the upgrade (Priority: P1)

An operator who installed sesh before this feature runs `sesh up` with
the new version. Open WebUI accounts, router keys, stored credentials
and documents are all still there, and every service works, now under
its own login. Nothing else is asked of the operator.

**Why this priority**: The household's data is the thing being
protected; an upgrade that loses it, or that needs manual database
work, is not acceptable.

**Independent Test**: Start a stack as it was before this feature
(services as the superuser, everything they created owned by it, one
shared cache), create an account, a router key, a credential and a
document tag; run the new `sesh up`; all of them are still there, each
database and everything in it belongs to its service's login, and the
services answer.

**Acceptance Scenarios**:

1. **Given** an install from before this feature, **When** the operator
   runs `sesh up`, **Then** any missing passwords are generated, each
   database and every object in it is handed to its service's login,
   and each service restarts with its own login.
2. **Given** the upgraded stack, **When** people use Open WebUI,
   LiteLLM, the secret store and Paperless, **Then** their earlier data
   is there and new data can be written.
3. **Given** an upgraded stack, **When** `sesh up` runs again, **Then**
   nothing changes.
4. **Given** the upgrade, **When** it finishes, **Then** only transient
   cache state is lost (live chat connections, queued background jobs
   and caches), and the quickstart says so.

---

### User Story 3 - Each service has its own cache (Priority: P1)

Open WebUI, Infisical and Paperless each use their own Redis, which
only they can reach, with its own password. One service can't read or
change another's sessions, queues or cache.

**Why this priority**: The shared Redis is a second path between
services that bypasses the Postgres isolation: queued jobs and cached
data cross it.

**Independent Test**: From each service, its own Redis is reachable
and the others' are not (by name or address); the gateway reaches none.

**Acceptance Scenarios**:

1. **Given** the running stack, **When** a service connects to its own
   Redis, **Then** it succeeds (the control).
2. **Given** the running stack, **When** a service tries another
   service's Redis, **Then** the name doesn't resolve and the address
   isn't reachable.

---

### User Story 4 - Doctor shows the isolation holds (Priority: P2)

`sesh doctor` reports whether any service is logged in to Postgres as
a superuser, and whether each service database belongs to its own
login with no administrative rights.

**Why this priority**: Constitution X: a guarantee that isn't checked
decays, for example when a service is added with the superuser's
connection string.

**Independent Test**: Doctor passes on a fresh and an upgraded stack,
and fails on the pre-upgrade stack, naming what is wrong.

**Acceptance Scenarios**:

1. **Given** the isolated stack, **When** doctor runs, **Then** the
   check passes.
2. **Given** a service connected as a superuser, or a service database
   owned by anyone but its login, **When** doctor runs, **Then** the
   check fails and says which, with the fix.

### Edge Cases

- **`sesh up` while services are running:** the hand-over takes each
  object's lock briefly; if a service holds a lock for over a minute,
  `sesh up` stops with the reason and can be run again.
- **A service created its own objects as the superuser before the
  upgrade** (tables, sequences, views, functions, types, schemas): all
  of them change hands; objects that belong to an extension stay with
  the extension.
- **A service needs a database extension:** extensions that need the
  superuser are created by the migration; none is needed today
  (research.md R4).
- **The old shared Redis:** its container is removed by `sesh up`; its
  volume is left for the operator to delete.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each adopted service that uses Postgres (Open WebUI,
  LiteLLM, Infisical, Paperless, Miniflux) MUST log in with its own
  login, never the superuser.
- **FR-002**: Each such login MUST own its service's database and MUST
  NOT be a superuser or be able to create databases or roles.
- **FR-003**: No login but the superuser MUST be able to connect to a
  database other than its own: connecting MUST be denied to everyone
  else on every sesh database.
- **FR-004**: Each login's password MUST be generated, kept only in
  `secrets.env`, and passed to Postgres as a psql variable on standard
  input, never in a file, a command line or a log.
- **FR-005**: `sesh up` MUST hand every service database and every
  object in it (schemas, tables, sequences, views, materialized views,
  functions, procedures, aggregates and types) to its service's login,
  idempotently, without moving anything outside that database.
- **FR-006**: The logins MUST exist and own their databases before any
  of their services starts, on a first install and on an upgrade.
- **FR-007**: `sesh up` MUST generate any missing passwords itself, so
  an upgrade is one command.
- **FR-008**: Open WebUI, Infisical and Paperless MUST each use their
  own Redis, reachable only from that service, with its own password.
- **FR-009**: The gateway's audit-log login (`sesh_gateway`) MUST keep
  exactly its current rights.
- **FR-010**: Doctor MUST fail when a service is connected to Postgres
  as a superuser from outside the Postgres container, when a service
  database isn't owned by its login, or when a service login has
  administrative rights.
- **FR-011**: Negative tests MUST cover: a login opening another
  database; a service reaching another's Redis; doctor on the
  pre-upgrade stack.

### Key Entities

- **Service login:** a Postgres role named after its service, with a
  generated password; owns that service's database.
- **Service cache:** a Redis instance per service, on a network shared
  only with that service.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the running stack, 100% of cross-database logins by
  service logins are refused.
- **SC-002**: After an upgrade, every record created before it is still
  readable, and 0 objects in a service database are owned by anyone
  but that service's login.
- **SC-003**: 0 services reach another service's Redis.
- **SC-004**: Doctor passes on fresh and upgraded stacks and fails on a
  pre-upgrade one.
- **SC-005**: No login password appears in any file other than
  `secrets.env`, in a log, or on a command line.

## Assumptions

- **Login names** are the service names, the same as their databases:
  `openwebui`, `litellm`, `infisical`, `paperless`, `miniflux`.
- **Passwords:** `OPENWEBUI_DB_PASSWORD`, `LITELLM_DB_PASSWORD`,
  `INFISICAL_DB_PASSWORD`, `PAPERLESS_DB_PASSWORD` (and Miniflux's
  existing `MINIFLUX_DB_PASSWORD`); `REDIS_OPENWEBUI_PASSWORD`,
  `REDIS_INFISICAL_PASSWORD`, `REDIS_PAPERLESS_PASSWORD`. The old
  shared `REDIS_PASSWORD` is no longer used.
- **Redis:** one small instance per service rather than one instance
  with access-control users (research.md R3).
- **Postgres stays shared:** one server, one data volume, one backup;
  the isolation is by login and database. Postgres stays on `default`.
- **The superuser** `sesh` remains, used only by `sesh up`, doctor and
  the operator, through `docker compose exec` inside the Postgres
  container.
- **The gateway database** stays owned by the superuser; the gateway
  keeps its insert-only login (feature 002).
- **Out of scope:** per-service Postgres networks (every service on
  `default` can still open a TCP connection to Postgres, and needs a
  password to get further); encrypting Redis traffic; the memory
  feature's `vector` extension.
- **Roadmap:** this is feature 007; read-only Google Docs becomes 008,
  memory 009, one agent 010, browser 011, household 012.
