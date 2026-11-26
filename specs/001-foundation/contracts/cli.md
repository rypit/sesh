# Contract: `sesh` CLI (this phase)

Exit codes: 0 success; 1 a check or operation failed; 2 usage error;
3 a prerequisite is missing (Docker not running, config missing).

| Command | Behavior | Spec refs |
|---|---|---|
| `sesh init [--inference-url URL --model NAME \| --stub] [--admin-email EMAIL]` | Create `$SESH_DATA_DIR` (0700), write `sesh.toml` if missing (flags fill it; flags given for an existing file update `[inference]`/`aliases.agent-main`), generate `secrets.env` values that are missing, render `litellm.yaml`. `--stub` sets the inference server to the stub service (`http://stub-llm:8000/v1`, model `stub-model`). Idempotent; prints what it wrote, never secret values. | FR-004, FR-007 |
| `sesh up` | Write the Compose files to `$SESH_DATA_DIR/compose/` (adding the stub overlay when the inference server is the stub); re-render `litellm.yaml` and restart only the router if it changed; start postgres, redis and litellm; create (first run) or update the Open WebUI router key limited to the aliases; start Open WebUI; wait for health. | FR-001, FR-003, FR-006 |
| `sesh down` | `docker compose down` (volumes kept). | FR-006 |
| `sesh doctor [--json]` | Run the checks below; print one line per check (status, detail, fix); exit 1 if any fails. | FR-009 |

## Doctor checks

| Id | Passes when | Fails / warns with |
|---|---|---|
| `alias-answers` | for each alias, the router lists it and a one-line chat returns content within the timeout | fail: alias missing, model missing upstream, timeout, error text |
| `alias-tools` | for each alias, a request offering one tool (`ping`, no arguments) returns a tool call to `ping` | fail: no tool call |
| `ports-loopback` | every published port of every stack service is bound to `127.0.0.1` or `::1` | fail: service, port and address |
| `services-healthy` | every service is running and its health check passes | fail: service and state |
| `tailscale-serve` | `tailscale serve status` shows the Open WebUI port served | warn: Tailscale missing or not serving it |
