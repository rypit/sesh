-- Idempotent migration for the gateway's audit log (specs/002-gateway-core).
-- Run by `sesh up` as the database owner through psql on stdin, after
-- `\set gateway_pw '...'` and `\set miniflux_pw '...'` lines; never logged.
\set ON_ERROR_STOP on

SELECT 'CREATE DATABASE gateway' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'gateway') \gexec
-- Infisical's own database (specs/004 research.md R5).
SELECT 'CREATE DATABASE infisical' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'infisical') \gexec
-- Paperless-ngx's own database (specs/005 research.md R6).
SELECT 'CREATE DATABASE paperless' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'paperless') \gexec
-- Miniflux's own database (specs/006 research.md R1).
SELECT 'CREATE DATABASE miniflux' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'miniflux') \gexec

DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'sesh_gateway') THEN
    CREATE ROLE sesh_gateway LOGIN;
  END IF;
END $$;
ALTER ROLE sesh_gateway PASSWORD :'gateway_pw';

-- Miniflux parses untrusted feeds, so it gets its own login that owns only
-- its own database (specs/006 research.md R2): never the superuser.
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'miniflux') THEN
    CREATE ROLE miniflux LOGIN;
  END IF;
END $$;
ALTER ROLE miniflux PASSWORD :'miniflux_pw' NOSUPERUSER NOCREATEDB NOCREATEROLE;
ALTER DATABASE miniflux OWNER TO miniflux;

REVOKE CONNECT ON DATABASE gateway FROM PUBLIC;
REVOKE CONNECT ON DATABASE openwebui FROM PUBLIC;
REVOKE CONNECT ON DATABASE litellm FROM PUBLIC;
REVOKE CONNECT ON DATABASE infisical FROM PUBLIC;
REVOKE CONNECT ON DATABASE paperless FROM PUBLIC;
REVOKE CONNECT ON DATABASE miniflux FROM PUBLIC;
GRANT CONNECT ON DATABASE gateway TO sesh_gateway;
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


-- Objects an earlier run created as the superuser belong to Miniflux too.
\c miniflux
ALTER SCHEMA public OWNER TO miniflux;
DO $$ DECLARE r record; BEGIN
  FOR r IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tableowner <> 'miniflux' LOOP
    EXECUTE format('ALTER TABLE public.%I OWNER TO miniflux', r.tablename);
  END LOOP;
  FOR r IN SELECT sequencename FROM pg_sequences WHERE schemaname = 'public' AND sequenceowner <> 'miniflux' LOOP
    EXECUTE format('ALTER SEQUENCE public.%I OWNER TO miniflux', r.sequencename);
  END LOOP;
END $$;
