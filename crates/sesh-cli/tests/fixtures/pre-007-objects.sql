-- Test fixture for isolation_stack (specs/007 research.md R7): one object
-- of each kind the hand-over must move, made by the superuser as an
-- install from before feature 007 would have. Run as the superuser in
-- each service database. `sesh_probe.notes` holds one row the test reads
-- back after the upgrade.
CREATE SCHEMA sesh_probe;
CREATE TYPE sesh_probe.mood AS ENUM ('ok', 'not');
CREATE DOMAIN sesh_probe.short AS text CHECK (length(VALUE) < 100);
CREATE TYPE sesh_probe.pair AS (a int, b text);
CREATE TYPE sesh_probe.span AS RANGE (subtype = int4);
CREATE TABLE sesh_probe.notes (
  id   serial PRIMARY KEY,
  n    bigint GENERATED ALWAYS AS IDENTITY,
  mood sesh_probe.mood,
  body sesh_probe.short
);
CREATE TABLE sesh_probe.parts (at date NOT NULL) PARTITION BY RANGE (at);
CREATE TABLE sesh_probe.parts_2026 PARTITION OF sesh_probe.parts
  FOR VALUES FROM ('2026-01-01') TO ('2027-01-01');
CREATE SEQUENCE sesh_probe.counter;
CREATE VIEW sesh_probe.recent AS SELECT * FROM sesh_probe.notes;
CREATE MATERIALIZED VIEW sesh_probe.counts AS SELECT count(*) AS n FROM sesh_probe.notes;
CREATE FUNCTION sesh_probe.touch() RETURNS trigger LANGUAGE plpgsql
  AS $$ BEGIN RETURN NEW; END $$;
CREATE TRIGGER notes_touch BEFORE UPDATE ON sesh_probe.notes
  FOR EACH ROW EXECUTE FUNCTION sesh_probe.touch();
CREATE PROCEDURE sesh_probe.tidy() LANGUAGE sql AS $$ SELECT 1 $$;
CREATE AGGREGATE sesh_probe.total(int) (sfunc = int4pl, stype = int);
CREATE TABLE public.sesh_probe_table (id serial PRIMARY KEY);
CREATE FUNCTION public.sesh_probe_fn() RETURNS int LANGUAGE sql AS $$ SELECT 1 $$;
-- An extension's objects stay the extension's.
CREATE EXTENSION IF NOT EXISTS pgcrypto;
INSERT INTO sesh_probe.notes (mood, body) VALUES ('ok', 'made before 007');
