# Data model: Service isolation

No new tables. What changes is who owns what.

## Postgres logins

| Role | Owns | Connects to | Attributes | Password (`secrets.env`) |
|---|---|---|---|---|
| `sesh` | `sesh`, `postgres`, `gateway` (and the audit tables) | everything (superuser) | superuser; used only through the Postgres container's local socket | `POSTGRES_PASSWORD` |
| `sesh_gateway` | nothing | `gateway` | unchanged (feature 002): insert on `audit`; insert, select, update on `approvals`, `credential_forms` | `GATEWAY_DB_PASSWORD` |
| `openwebui` | database `openwebui` and everything in it | `openwebui` | `NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS` | `OPENWEBUI_DB_PASSWORD` |
| `litellm` | database `litellm` and everything in it | `litellm` | as above | `LITELLM_DB_PASSWORD` |
| `infisical` | database `infisical` and everything in it | `infisical` | as above | `INFISICAL_DB_PASSWORD` |
| `paperless` | database `paperless` and everything in it | `paperless` | as above | `PAPERLESS_DB_PASSWORD` |
| `miniflux` | database `miniflux` and everything in it | `miniflux` | as above (feature 006) | `MINIFLUX_DB_PASSWORD` |

CONNECT is revoked from PUBLIC on `sesh`, `postgres`, `gateway` and
every service database.

## Redis instances

| Service | Client | Network (internal, two members) | Volume | Password (`secrets.env`) |
|---|---|---|---|---|
| `redis-openwebui` | Open WebUI (`REDIS_URL`, websockets) | `openwebui-redis` | `redis-openwebui-data` | `REDIS_OPENWEBUI_PASSWORD` |
| `redis-infisical` | Infisical (`REDIS_URL`, queues and cache) | `infisical-redis` | `redis-infisical-data` | `REDIS_INFISICAL_PASSWORD` |
| `redis-paperless` | Paperless (`PAPERLESS_REDIS`, Celery broker) | `paperless-redis` | `redis-paperless-data` | `REDIS_PAPERLESS_PASSWORD` |

Removed: the shared `redis` service. Its volume `redis-data` is left
in place on existing installs; `REDIS_PASSWORD` is no longer generated
or used.

## Doctor check

| Id | Pass | Fail |
|---|---|---|
| `db-isolation` | no superuser session from outside the Postgres container; each service database owned by its role; no service role with administrative rights | each problem, named; fix: `sesh up` |
