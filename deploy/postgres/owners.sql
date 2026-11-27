-- Hands a service database, and everything in it, to the database's owner
-- (specs/007 research.md R2, ADR 0010). `sesh up` runs it as the superuser
-- connected to each service database in turn, on every run. Only objects
-- owned by anyone else are changed, so a second run changes nothing and
-- takes no locks. It never reassigns everything a role owns, which would
-- also move objects outside this database (the `sesh` and `gateway`
-- databases among them).
--
-- Left alone: the system schemas; objects that belong to an extension
-- (dependency type 'e'); array types and a table's or view's row type,
-- which follow their base type or relation; sequences linked to a table
-- column ('a' or 'i' on the table), which Postgres moves with their table
-- and won't move alone. Indexes, TOAST tables and triggers follow their
-- table.
SET lock_timeout = '1min';
DO $$
DECLARE
  target name;
  target_oid oid;
  r record;
BEGIN
  SELECT d.datdba, pg_get_userbyid(d.datdba) INTO target_oid, target
    FROM pg_database d WHERE d.datname = current_database();
  IF (SELECT rolsuper FROM pg_roles WHERE oid = target_oid) THEN
    RAISE EXCEPTION 'database % is owned by a superuser (%), not its service''s role',
      current_database(), target;
  END IF;

  -- Schemas; `public` belongs to pg_database_owner, which already means
  -- the database's owner.
  FOR r IN
    SELECT n.nspname FROM pg_namespace n
    WHERE n.nspowner NOT IN (target_oid, 'pg_database_owner'::regrole::oid)
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.classid = 'pg_namespace'::regclass
                      AND d.objid = n.oid AND d.deptype = 'e')
  LOOP
    EXECUTE format('ALTER SCHEMA %I OWNER TO %I', r.nspname, target);
  END LOOP;

  -- Enum, range, multirange, domain and stand-alone composite types.
  FOR r IN
    SELECT t.oid::regtype AS name, t.typtype FROM pg_type t
    JOIN pg_namespace n ON n.oid = t.typnamespace
    WHERE t.typowner <> target_oid
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND (t.typtype IN ('e', 'r', 'm', 'd')
           OR (t.typtype = 'c' AND (SELECT c.relkind FROM pg_class c WHERE c.oid = t.typrelid) = 'c'))
      AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.classid = 'pg_type'::regclass
                      AND d.objid = t.oid AND d.deptype = 'e')
  LOOP
    EXECUTE format('ALTER %s %s OWNER TO %I',
      CASE r.typtype WHEN 'd' THEN 'DOMAIN' ELSE 'TYPE' END, r.name, target);
  END LOOP;

  -- Tables (and partitions), views, materialized views, foreign tables.
  FOR r IN
    SELECT c.oid::regclass AS name, c.relkind FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relowner <> target_oid AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.classid = 'pg_class'::regclass
                      AND d.objid = c.oid AND d.deptype = 'e')
  LOOP
    EXECUTE format('ALTER %s %s OWNER TO %I',
      CASE r.relkind WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW'
                     WHEN 'f' THEN 'FOREIGN TABLE' ELSE 'TABLE' END,
      r.name, target);
  END LOOP;

  -- Sequences no table column owns (the others moved with their table).
  FOR r IN
    SELECT c.oid::regclass AS name FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relowner <> target_oid AND c.relkind = 'S'
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.classid = 'pg_class'::regclass
                      AND d.objid = c.oid AND (d.deptype = 'e'
                        OR (d.deptype IN ('a', 'i') AND d.refclassid = 'pg_class'::regclass
                            AND d.refobjid <> c.oid)))
  LOOP
    EXECUTE format('ALTER SEQUENCE %s OWNER TO %I', r.name, target);
  END LOOP;

  -- Functions, procedures and aggregates (a range type's constructors too).
  FOR r IN
    SELECT p.oid::regprocedure AS name FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE p.proowner <> target_oid
      AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
      AND NOT EXISTS (SELECT FROM pg_depend d WHERE d.classid = 'pg_proc'::regclass
                      AND d.objid = p.oid AND d.deptype = 'e')
  LOOP
    EXECUTE format('ALTER ROUTINE %s OWNER TO %I', r.name, target);
  END LOOP;
END $$;
