-- Test fixture for isolation_stack (specs/007 research.md R7): puts a
-- freshly started stack's Postgres back the way an install from before
-- feature 007 had it. The four services' databases, and everything in
-- them, belong to the superuser again, and their logins are gone
-- (Miniflux had its own since feature 006). Run as the superuser, with
-- the four services stopped. Test-only: sesh's own migration never
-- reassigns everything a role owns (specs/007 research.md R2).
\set ON_ERROR_STOP on
\c openwebui
REASSIGN OWNED BY openwebui TO sesh;
DROP OWNED BY openwebui;
\c litellm
REASSIGN OWNED BY litellm TO sesh;
DROP OWNED BY litellm;
\c infisical
REASSIGN OWNED BY infisical TO sesh;
DROP OWNED BY infisical;
\c paperless
REASSIGN OWNED BY paperless TO sesh;
DROP OWNED BY paperless;
\c sesh
DROP ROLE openwebui;
DROP ROLE litellm;
DROP ROLE infisical;
DROP ROLE paperless;
