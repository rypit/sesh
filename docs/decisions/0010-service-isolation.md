# 0010. A database login and a cache per adopted service

- Status: proposed
- Date: 2026-11-27
- Feature: specs/007-service-isolation

## Context

sesh's adopted services are untrusted (constitution I): each parses
input that people, agents or the internet chose (chat messages, model
output, documents, feeds). Until now most of them shared sesh's storage
with full rights:

- Open WebUI, LiteLLM, Infisical and Paperless logged in to the shared
  Postgres as the superuser `sesh`. Any of them, compromised, could read
  and rewrite every database: the gateway's audit log and approvals
  (whose integrity constitution X relies on), Infisical's encrypted
  secrets and machine identities, every other service's data. A
  superuser can also create roles and run programs on the Postgres
  server.
- Open WebUI, Infisical and Paperless shared one Redis with one
  password, on database indexes 0, 1 and 2. Redis's `SELECT` lets any
  client use every index, so each could read or forge the others'
  sessions, queued jobs and cache entries, for example a Celery task
  for Paperless.

Only the gateway (an insert-only audit login, feature 002) and Miniflux
(its own login, feature 006) were already confined.

## Decision

1. **A Postgres login per service** that stores data: `openwebui`,
   `litellm`, `infisical`, `paperless` and `miniflux`, named after the
   service and its database. Each owns its database and nothing else,
   and has `NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION
   NOBYPASSRLS`. CONNECT is revoked from PUBLIC on every sesh database
   and granted only to each database's owner. Passwords are generated
   into `secrets.env` and reach psql only as variables on stdin.
2. **The superuser is for sesh's own admin tasks only**: `sesh up`'s
   migration, doctor and the operator, through the Postgres
   container's local socket. No connection string names it.
3. **Existing installs are handed over in place** by the migration that
   `sesh up` runs before any service starts: each database's owner,
   then every schema, table, sequence, view, materialized view,
   function, procedure, aggregate and type in it, except extension
   members, to its role. It is idempotent and never uses `REASSIGN
   OWNED`, which would also move objects outside that database, such
   as the `sesh` and `gateway` databases.
4. **A Redis per service**: `redis-openwebui`, `redis-infisical` and
   `redis-paperless`, each with its own password, on an internal network
   shared only with its client, and on no other network. The shared
   `redis` is removed.
5. **Doctor's `db-isolation`** fails on a superuser session from outside
   the Postgres container, a service database owned by anyone but its
   login, or a service login with administrative rights.
6. **`sesh_gateway` keeps exactly its rights**; the `gateway` database
   stays the superuser's.

## Alternatives

- **Redis ACL users with key patterns, on one instance.** ACLs can't
  confine a user to a database index, so isolation would rest on key
  and channel patterns that the apps don't keep to (BullMQ, Celery,
  Socket.IO, ad hoc cache keys) and that any app upgrade could change,
  breaking it at runtime or widening it silently
  (specs/007 research.md R3). Rejected.
- **A Postgres server per service.** Stronger (no shared server, no
  shared superuser), but five servers to tune, back up and upgrade, for
  a household. Logins and database ownership give the isolation that
  matters; a Postgres privilege-escalation bug is the remaining risk.
- **`REASSIGN OWNED BY sesh TO <role>`** for the hand-over: moves the
  `sesh` and `gateway` databases and anything else `sesh` owns.
  Rejected.
- **Per-service Postgres networks** (as Miniflux's `feeds-db`). Every
  service on `default` can still open a TCP connection to Postgres; with
  its own login it gets no further than its own database. Left for
  later, as defence in depth.

## Consequences

- **Threat model:** a compromised adopted service now reaches its own
  database and its own cache, and can't read or change the audit log,
  the secret store's database, another service's data, or another
  service's queue. It still shares a Postgres server with the others
  (a server bug or resource exhaustion affects all), and still reaches
  Postgres and the other services on `default` over the network, where
  each needs its own credentials.
- **Upgrade:** `sesh up` generates the new passwords and hands
  ownership over; only transient Redis state is lost (live websockets,
  queued jobs, caches). The old `redis-data` volume is left for the
  operator to remove.
- **Extensions:** none of the services needs one now. One that needs the
  superuser (such as `vector` for the memory feature) must be created by
  the migration, not by the service.
- **Adding a service** that uses Postgres or Redis means adding its role
  to `SERVICE_DATABASES` and `gateway.sql`, or its own Redis instance
  and network; doctor and the unit tests catch a connection string that
  uses the superuser.
- Two more small containers (a few MB of memory each).
