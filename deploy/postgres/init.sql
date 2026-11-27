-- Databases for the foundation stack, made as the superuser on first boot;
-- later phases add theirs in gateway.sql, which `sesh up` runs before any
-- service starts and which hands each database to its own login (specs/007).
CREATE DATABASE openwebui;
CREATE DATABASE litellm;
