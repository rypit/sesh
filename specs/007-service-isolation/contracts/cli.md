# Contract: CLI changes

No new commands.

`sesh up`:
- generates any missing value it needs in `secrets.env` before it
  starts (names printed as `generated <NAME>`, never values), so an
  existing install upgrades with `sesh up` alone;
- runs the migration (roles, database owners, CONNECT, then the
  hand-over in each service database) before any service starts;
- removes the old shared `redis` container (Compose's orphan removal).

`sesh doctor` gains `db-isolation`:
- pass: "every service logs in as its own role, owning only its
  database";
- fail, listing each problem:
  - `superuser session from <address> (<application>)`;
  - `database <db> is owned by <role>` (or `is missing`);
  - `role <role> has administrative rights`;
  with the fix `run \`sesh up\``.

`sesh init` generates the new passwords, like every other generated
value.
