# Research: Service isolation

## R0. Where things stood

| Service | Postgres login | Database | Redis |
|---|---|---|---|
| Open WebUI | `sesh` (superuser) | `openwebui` | shared `redis`, db 0 |
| LiteLLM | `sesh` (superuser) | `litellm` | none |
| Infisical | `sesh` (superuser) | `infisical` | shared `redis`, db 1 |
| Paperless | `sesh` (superuser) | `paperless` | shared `redis`, db 2 |
| Miniflux | `miniflux` (feature 006) | `miniflux` | none |
| Gateway | `sesh_gateway` (insert-only audit, feature 002) | `gateway` | none |

- A superuser can read and change every database, including the
  gateway's audit log (constitution I, X) and Infisical's encrypted
  secrets and identities, can create roles, and can run programs on the
  Postgres server (`COPY … PROGRAM`).
- One Redis password and Redis's `SELECT` mean every client of the
  shared Redis can read and write every database index in it.

## R1. A login per service

- **Decision:** a role per service named after it and its database:
  `openwebui`, `litellm`, `infisical`, `paperless` (and the existing
  `miniflux`). `LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION
  NOBYPASSRLS`, with a generated password.
- **Owns its database.** The role is the database's owner, so it can
  create and change what it needs there: since Postgres 15 the `public`
  schema belongs to `pg_database_owner`, which is whoever owns the
  database, so the role can create tables in `public` without any grant.
- **CONNECT.** It is revoked from PUBLIC on every database sesh creates
  and on `sesh` and `postgres` (already done in features 002 and 006),
  and granted to each database's own role only. `template0` and
  `template1` are left as Postgres ships them: a service login can open
  `template1` but can create nothing in it (its `public` schema grants
  no CREATE to PUBLIC since Postgres 15) and finds nothing there.
- **Passwords** (`OPENWEBUI_DB_PASSWORD`, `LITELLM_DB_PASSWORD`,
  `INFISICAL_DB_PASSWORD`, `PAPERLESS_DB_PASSWORD`, and
  `MINIFLUX_DB_PASSWORD`) are generated into `secrets.env` and passed to
  psql as variables on standard input, as the gateway's and Miniflux's
  already are. `sesh up` refuses a password containing a quote, a
  backslash or a line break, so a hand-edited value can't break out of
  the psql variable.
- **The superuser stays** for `sesh up`'s migration, doctor and the
  operator, all through `docker compose exec` over the Postgres
  container's local socket. No service's connection string names it.
- **`sesh_gateway` is unchanged**: insert on `audit`, insert, select
  and update on `approvals` and `credential_forms`; the `gateway`
  database stays the superuser's.

## R2. Order, and handing over an existing install

- **First boot:** the Postgres image runs `init.sql` once, which creates
  `openwebui` and `litellm` as the superuser.
- **Every `sesh up`:** `sesh up` starts Postgres alone, runs the
  migration (`gateway.sql`), and only then starts the other services.
  So the migration always runs before any service connects, and that
  is where the roles are made.
- **`gateway.sql`, in order:** create each role if missing and set its
  password and attributes; create each database if missing (now
  `openwebui` and `litellm` too, so nothing depends on `init.sql`
  having run); `ALTER DATABASE … OWNER TO` its role; revoke CONNECT
  from PUBLIC; grant CONNECT to the owner.
- **Then, in each service database** (`sesh up` appends `\c <db>` and
  `deploy/postgres/owners.sql` for each one), one `DO` block hands every
  object to the database's owner. It reads the owner from `pg_database`,
  so the same text serves every database, and refuses to run if the
  owner is a superuser. It changes, outside `pg_catalog`,
  `information_schema` and `pg_toast`/`pg_temp`:
  - schemas (except those owned by `pg_database_owner`, which already
    means the database's owner);
  - tables and partitions, views, materialized views and foreign tables
    (indexes, TOAST tables and triggers follow their table);
  - sequences, except those linked to a table column (`serial`,
    identity), which move with their table: Postgres refuses to change
    a linked sequence on its own;
  - functions, procedures and aggregates (window functions too);
  - enum, range, domain and stand-alone composite types (array and
    multirange types follow their base type; a table's row type
    follows the table).
  Objects that belong to an extension are left alone. Only objects
  whose owner differs are touched, so a second run changes nothing and
  takes no locks.
- **Not `REASSIGN OWNED BY sesh`:** it would also hand over objects
  outside the database being migrated and shared objects, starting with
  the `sesh` and `gateway` databases and the audit tables, to whichever
  role it names.
- **Locks:** `ALTER … OWNER` takes an exclusive lock on each object.
  On an upgrade the old services are still connected, as the
  superuser, until `sesh up` restarts them a moment later; their
  queries wait briefly. `lock_timeout` is one minute, so a service
  holding a lock for longer makes `sesh up` stop with Postgres's reason
  instead of hanging; running it again continues (the block is
  idempotent).
- **Why it matters beyond the first run:** LiteLLM runs `CREATE OR
  REPLACE VIEW` for its spend views at every start, and Infisical's
  migrations run `CREATE OR REPLACE FUNCTION on_update_timestamp()`.
  Both need the role to own the existing view or function, not just
  the tables.
- **Not covered** (no service creates them): large objects, operators,
  collations, text-search configurations, statistics objects, foreign
  servers, publications, event triggers. Anything a service creates
  after the upgrade belongs to its role, which created it; only an
  object of these kinds made before the upgrade would stay the
  superuser's.

## R3. Redis: an instance per service

| Option | For | Against |
|---|---|---|
| **One instance, an ACL user per service** with key and channel patterns | One container | ACLs can't tie a user to a database index: `SELECT` is all or nothing, so isolation would rest on key prefixes alone. The clients don't keep to one prefix: Infisical's BullMQ queues use `bull:*` but its cache and locks use many unprefixed key names; Celery (Paperless's broker) uses `celery`, `unacked*` and `_kombu.binding.*` keys and fan-out channels; Open WebUI's Socket.IO manager uses pub/sub channels next to its `open-webui:*` keys, and BullMQ and Socket.IO run Lua scripts (`EVALSHA`). Each upgrade of an app could add a key outside its pattern and fail at runtime with `NOPERM`, or need a wider pattern. |
| **An instance per service, each on an internal network shared only with its client** | Nothing to keep in step with the apps; a compromised service can't even open a connection to another's cache; each has its own password | Three containers instead of one: a few MB of memory each when idle |

- **Decision:** an instance per service: `redis-openwebui`,
  `redis-infisical`, `redis-paperless`, the same pinned
  `redis:7.4-alpine` image, each with `--requirepass` (its own
  generated password) and `--appendonly yes` on its own volume, and on
  one internal network shared only with its client (`openwebui-redis`,
  `infisical-redis`, `paperless-redis`). None is on `default`, so the
  gateway, executors and the other services can't reach any of them.
- Each client uses database 0 of its own instance.
- Persistence stays on, so Paperless's queued consumption tasks survive
  a restart.

## R4. Extensions

- **Searched:** each project's source for `CREATE EXTENSION` and for
  Django's and Prisma's extension operations (the projects' current
  sources; the pinned releases are recent).
  - **LiteLLM** (Prisma schema and migrations, its view scripts): none.
    Prisma's `migrate deploy` needs no shadow database (only `migrate
    dev` does) and its advisory lock needs no rights.
  - **Open WebUI:** only in its optional pgvector vector store
    (`vector`, `pgcrypto`), which sesh doesn't enable: its vector store
    is the default local one.
  - **Infisical** (knex migrations): none; it creates plain functions
    and triggers, which an owner may.
  - **Paperless-ngx** (Django migrations): none.
  - **Miniflux** (feature 006): drops `hstore` if present, never creates
    one.
- **Decision:** the migration creates no extension now. Since Postgres
  13, trusted extensions (`pgcrypto`, `uuid-ossp`, `pg_trgm`, `citext`,
  `hstore`, …) can be created by a database's owner without the
  superuser anyway. Extensions already created in a database stay with
  their owner and keep working (their functions are executable by
  PUBLIC).
- **`vector`** (pgvector, in the image already) isn't trusted, so the
  memory feature (009) will create it as the superuser in its own
  database's migration.

## R5. The upgrade

- **Passwords:** `sesh up` now generates any missing generated value
  (the new logins' and caches' passwords) before it starts, and says
  which (names only), so an upgrade is `sesh up` alone. `sesh init`
  does the same.
- **Postgres:** `gateway.sql` and the hand-over run with the old
  services still up; `sesh up` then recreates Open WebUI, LiteLLM,
  Infisical and Paperless, whose connection strings changed. All data
  stays where it was: only owners change.
- **Redis:** the old `redis` service is no longer in the Compose file,
  so `sesh up` (`--remove-orphans`) removes its container; the new
  instances start empty. What is lost is transient:
  - Open WebUI: live websocket sessions (open tabs reconnect; sign-ins
    are tokens signed with `WEBUI_SECRET_KEY`, so nobody is signed
    out);
  - Infisical: queued background jobs and its cache, which it rebuilds;
  - Paperless: queued Celery tasks, such as a document being consumed
    at that moment. Task results are in Postgres and stay. Upgrade when
    no upload is in flight; re-upload anything missing.
- **The old volume** `sesh_redis-data` is kept: Compose doesn't delete
  a volume it no longer manages. The quickstart tells the operator to
  remove it (`docker volume rm sesh_redis-data`).
- **`REDIS_PASSWORD`** stays in an existing `secrets.env`, unused; new
  installs don't generate it.

## R6. Doctor: `db-isolation`

- One query, as the superuser, through the same `docker compose exec`
  doctor already uses for the audit check, that returns only problems:
  - a session of a superuser role with a client address (a TCP
    connection, so from another container: `sesh up`, doctor and the
    operator use the local socket inside the Postgres container, which
    has none), with the address and application name;
  - a service database missing, or owned by anyone but its role;
  - a service role that is a superuser or can create databases or
    roles.
- **Fail** when anything is returned: this is a safety check. The fix is
  `sesh up`, which reapplies the migration and the connection strings.
- **Why both parts:** connections show a misconfigured service only
  while it is connected (Paperless's web workers close theirs after
  each request); ownership and attributes show the database side at any
  time. A unit test also checks that no connection string in
  `deploy/compose.yaml` names the superuser.

## R7. Tests

- **Unit:** the migration has, for each service, its role, its psql
  password variable, its database owner, its CONNECT grant and its
  hand-over block, and no password; `migration_sql` puts every password
  in a `\set` line only and refuses unsafe ones; the Compose file gives
  every service its own login and Redis, and none the superuser's;
  doctor's check passes on no output and fails, naming the problem,
  otherwise; `sesh up` generates missing passwords.
- **Stack (`isolation_stack`):**
  1. a fresh `sesh up` with an agent using the test credential
     connector;
  2. **back to the pre-007 state:** stop the four services, run
     `tests/fixtures/pre-007.sql` as the superuser (hand everything back
     to `sesh` and drop the four roles), then `pre-007-objects.sql` in
     each of the four databases (probe objects of every kind the
     hand-over must move, made by `sesh`), and start them with
     `tests/fixtures/pre-007.compose.yaml` (superuser connection strings
     and one shared `redis`);
  3. doctor's check fails there; create an Open WebUI user, a router
     key, a stored credential and a Paperless tag; drop the new
     passwords from `secrets.env`, as a pre-007 install lacks them;
  4. the new `sesh up`, which generates them; everything from step 3 is still there and
     works, new data can be written, every object in each service
     database belongs to its role, the old `redis` is gone, doctor
     passes;
  5. each login reaches only its own database; each service reaches
     only its own Redis (with its own as the control), by name and by
     address, and the gateway reaches none.

- **Checked locally without Docker:** the migration was run against a
  Postgres 16 server: the rss-feeds migration, then the probe objects
  as the superuser in every service database, then the new migration.
  Every object changed hands (a first draft missed the partitioned
  parent, whose partition key depends on the table itself, and a range
  type's multirange and constructors; both are fixed and covered by the
  probe objects); a second run changed nothing; each login opened only
  its own database, could replace its views and functions, refresh its
  materialized views and create a trusted extension, and could not
  create a database, a role or `vector`; the pre-007 fixture rewound
  the state and the migration handed it over again; doctor's query
  listed the four superuser-owned databases and a superuser TCP
  session, and nothing afterwards.

## R8. Boundary change → ADR 0010

- What talks to what changes (each service's own Redis, on its own
  network) and the threat model changes (a compromised service no
  longer reaches other databases or caches): ADR 0010.
- The architecture diagram doesn't draw Postgres or Redis, so it
  doesn't change; the catalog row, deployment table and phase list do.
- No principle changes: this enforces I and X for storage.

## CI results (T015)

Run 46039623223 (commit 469ae93), the first run of this feature,
passed every job: `check`, `images`, and all seven stack tests:
`foundation_stack`, `gateway_stack`, `eval_stack`, `secrets_stack`,
`google_docs_stack`, `feeds_stack` and the new `isolation_stack`.

So, with each service on its own Postgres login and its own Redis:
- Open WebUI, LiteLLM, Infisical and Paperless all start and migrate as
  non-superusers;
- every earlier feature's stack test still passes;
- `isolation_stack` passes both a fresh install and the upgrade from the
  pre-007 state: data in all four services survives `sesh up`, a second
  run changes nothing, and each login and each Redis reaches only its own.

The upgrade risks recorded above that CI doesn't exercise remain: lock
waits while old services are still connected, and the small window for
an object created by an old superuser connection.
