-- Idempotent migration for the gateway's audit log (specs/002-gateway-core).
-- Run by `sesh up` as the database owner through psql on stdin, after a
-- `\set gateway_pw '...'` line; never logged.
\set ON_ERROR_STOP on

SELECT 'CREATE DATABASE gateway' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'gateway') \gexec

DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'sesh_gateway') THEN
    CREATE ROLE sesh_gateway LOGIN;
  END IF;
END $$;
ALTER ROLE sesh_gateway PASSWORD :'gateway_pw';

REVOKE CONNECT ON DATABASE gateway FROM PUBLIC;
REVOKE CONNECT ON DATABASE openwebui FROM PUBLIC;
REVOKE CONNECT ON DATABASE litellm FROM PUBLIC;
GRANT CONNECT ON DATABASE gateway TO sesh_gateway;

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
