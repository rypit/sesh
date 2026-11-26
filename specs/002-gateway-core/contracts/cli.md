# Contract: CLI additions

| Command | Behavior |
|---|---|
| `sesh agent add <name> --connectors a,b` | Generate a token (stored in `agents/<name>.token`, 0600, never printed), add the hashed record to `agents.toml`, then `sesh up` applies it (executor and network created, gateway reloads). |
| `sesh agent remove <name>` | Remove the record and token file; `sesh up` removes the executor; the token is refused. |
| `sesh agent list` | Names and connectors (no tokens). |
| `sesh init --home LAT,LON` | Sets the gateway's home location. |
| `sesh doctor` | Adds `gateway-healthy`, `executors-healthy` (one per agent), `audit-refusals` (warn when an agent had ≥ 10 refusals in the last hour). |
| `sesh up` | Adds: migrate the `gateway` database and role; render `compose/agents.yaml`; start gateway, searxng, executors (and kiwix when ZIM files exist). |
