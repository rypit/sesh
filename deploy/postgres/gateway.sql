-- Idempotent migration (specs/002-gateway-core, 006, 007). Run by `sesh up`
-- as the superuser through psql on stdin, before any service starts, after
-- `\set gateway_pw '...'` and one `\set <role>_pw '...'` line per service
-- role; never logged. `sesh up` then appends, for each service database,
-- `\c <database>` and deploy/postgres/owners.sql, which hands everything in
-- it to the database's owner.
\set ON_ERROR_STOP on

-- A login per adopted service, named after it and its database, that owns
-- only that database: never the superuser (specs/007 research.md R1,
-- ADR 0010). Miniflux's came first (specs/006 research.md R2).
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'openwebui') THEN
    CREATE ROLE openwebui LOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'litellm') THEN
    CREATE ROLE litellm LOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'infisical') THEN
    CREATE ROLE infisical LOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'paperless') THEN
    CREATE ROLE paperless LOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'miniflux') THEN
    CREATE ROLE miniflux LOGIN;
  END IF;
END $$;
ALTER ROLE openwebui LOGIN PASSWORD :'openwebui_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE litellm LOGIN PASSWORD :'litellm_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE infisical LOGIN PASSWORD :'infisical_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE paperless LOGIN PASSWORD :'paperless_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
ALTER ROLE miniflux LOGIN PASSWORD :'miniflux_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;

-- The gateway's audit log (specs/002), owned by the superuser.
SELECT 'CREATE DATABASE gateway' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'gateway') \gexec
-- Each service's database. Open WebUI's and LiteLLM's are also made by
-- init.sql on first boot, as the superuser; they are handed over below.
SELECT 'CREATE DATABASE openwebui OWNER openwebui' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'openwebui') \gexec
SELECT 'CREATE DATABASE litellm OWNER litellm' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'litellm') \gexec
-- Infisical (specs/004 research.md R5).
SELECT 'CREATE DATABASE infisical OWNER infisical' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'infisical') \gexec
-- Paperless-ngx (specs/005 research.md R6).
SELECT 'CREATE DATABASE paperless OWNER paperless' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'paperless') \gexec
-- Miniflux (specs/006 research.md R1).
SELECT 'CREATE DATABASE miniflux OWNER miniflux' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'miniflux') \gexec

-- Databases an earlier version created as the superuser change hands.
ALTER DATABASE openwebui OWNER TO openwebui;
ALTER DATABASE litellm OWNER TO litellm;
ALTER DATABASE infisical OWNER TO infisical;
ALTER DATABASE paperless OWNER TO paperless;
ALTER DATABASE miniflux OWNER TO miniflux;

DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'sesh_gateway') THEN
    CREATE ROLE sesh_gateway LOGIN;
  END IF;
END $$;
ALTER ROLE sesh_gateway PASSWORD :'gateway_pw';

-- Only a database's own login may connect to it (and the superuser).
REVOKE CONNECT ON DATABASE gateway FROM PUBLIC;
REVOKE CONNECT ON DATABASE openwebui FROM PUBLIC;
REVOKE CONNECT ON DATABASE litellm FROM PUBLIC;
REVOKE CONNECT ON DATABASE infisical FROM PUBLIC;
REVOKE CONNECT ON DATABASE paperless FROM PUBLIC;
REVOKE CONNECT ON DATABASE miniflux FROM PUBLIC;
GRANT CONNECT ON DATABASE gateway TO sesh_gateway;
GRANT CONNECT ON DATABASE openwebui TO openwebui;
GRANT CONNECT ON DATABASE litellm TO litellm;
GRANT CONNECT ON DATABASE infisical TO infisical;
GRANT CONNECT ON DATABASE paperless TO paperless;
GRANT CONNECT ON DATABASE miniflux TO miniflux;
-- The default databases too: a service login reaches only its own.
REVOKE CONNECT ON DATABASE sesh FROM PUBLIC;
REVOKE CONNECT ON DATABASE postgres FROM PUBLIC;

\c gateway

CREATE TABLE IF NOT EXISTS audit (
  id           bigserial PRIMARY KEY,
  at           timestamptz NOT NULL DEFAULT now(),
  agent        text NOT NULL,
  execution    text,
  connector    text,
  method       text,
  args_summary jsonb NOT NULL DEFAULT '{}'::jsonb,
  outcome      text NOT NULL CHECK (outcome IN ('allowed', 'refused', 'failed')),
  reason       text
);
CREATE INDEX IF NOT EXISTS audit_agent_at ON audit (agent, at);

REVOKE ALL ON audit FROM PUBLIC;
REVOKE ALL ON audit FROM sesh_gateway;
GRANT INSERT ON audit TO sesh_gateway;
GRANT USAGE ON SEQUENCE audit_id_seq TO sesh_gateway;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- Approvals and one-time credential forms (specs/004 data-model.md). The
-- gateway may insert, read and update them, never delete; only secret
-- hashes are stored.
CREATE TABLE IF NOT EXISTS approvals (
  id            text PRIMARY KEY,
  agent         text NOT NULL,
  connector     text NOT NULL,
  method        text NOT NULL,
  fingerprint   text NOT NULL,
  shown_text    text NOT NULL,
  secret_sha256 text NOT NULL,
  created_at    bigint NOT NULL,
  expires_at    bigint NOT NULL,
  state         text NOT NULL CHECK (state IN ('pending', 'approved', 'denied', 'expired', 'used')),
  reason        text,
  decided_at    bigint,
  used_at       bigint
);
CREATE INDEX IF NOT EXISTS approvals_agent_fp ON approvals (agent, fingerprint, created_at);

CREATE TABLE IF NOT EXISTS credential_forms (
  id            text PRIMARY KEY,
  connector     text NOT NULL,
  name          text NOT NULL,
  secret_sha256 text NOT NULL,
  created_at    bigint NOT NULL,
  expires_at    bigint NOT NULL,
  used_at       bigint
);

REVOKE ALL ON approvals, credential_forms FROM PUBLIC;
REVOKE ALL ON approvals, credential_forms FROM sesh_gateway;
GRANT INSERT, SELECT, UPDATE ON approvals, credential_forms TO sesh_gateway;

-- Memory (specs/011, ADR 0015): facts agents saved, one owner each, taken
-- from the agent's token. pgvector is in the Postgres image; creating the
-- extension needs the superuser, which runs this migration.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE TABLE IF NOT EXISTS memories (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  owner       text NOT NULL,
  agent       text NOT NULL,
  fact        text NOT NULL CHECK (char_length(fact) BETWEEN 1 AND 500),
  embedding   vector(384),
  tsv         tsvector GENERATED ALWAYS AS (to_tsvector('simple', fact)) STORED,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS memories_owner_updated ON memories (owner, updated_at DESC);
CREATE INDEX IF NOT EXISTS memories_tsv ON memories USING gin (tsv);
CREATE INDEX IF NOT EXISTS memories_embedding ON memories USING hnsw (embedding vector_cosine_ops);
REVOKE ALL ON memories FROM PUBLIC;
REVOKE ALL ON memories FROM sesh_gateway;
GRANT SELECT, INSERT, UPDATE, DELETE ON memories TO sesh_gateway;
