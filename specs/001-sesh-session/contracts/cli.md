# Contract: `sesh` CLI

Exit codes for every command:

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | general error |
| 2 | usage error |
| 3 | platform or prerequisite not met (doctor failure) |
| 4 | security check failed (policy mismatch, invariant failure); sesh refused to proceed |

## Commands

| Command | Behavior | Spec refs |
|---|---|---|
| `sesh [--profile online\|offline] [--new]` | In a git repo: ensure the stack is up, prepare the isolated workspace, apply policy and secrets, open pi in the repo's sandbox. Continues the current session unless `--new` starts a fresh session id on the existing sandbox. Refuses (exit 1) if a harness is already running in this repo's sandbox. | FR-001–FR-011, FR-022, FR-026, FR-027, US1 |
| `sesh pull [--session ID]` | Bundle the agent's commits inside the sandbox and fetch them into the host checkout as `sesh/<session>/<branch>`. Lists the carried-changes commit separately from the agent's commits, records what was pulled, and reports uncommitted work in the sandbox, offering to commit it there first. | FR-011, FR-012, US2 |
| `sesh verify [--only ID…]` | Run the invariant suite inside a sandbox; print a PASS/FAIL table with expected vs observed for failures. | FR-021, US4 |
| `sesh policy show [--profile …]` | Print the active profile and the compiled policy for the current repo's sandbox: endpoints, method/path rules, credential bindings (names and hosts only). | FR-019, FR-024 |
| `sesh init` | One-time setup: check the platform, install components, download the default model, authenticate sbx, build the pi kit, write the default config if missing. Idempotent. | FR-023, US5 |
| `sesh doctor` | Check the platform, component versions, config validity and stack health; print each check. No changes. | FR-022 |
| `sesh stack up\|down\|status` | Manage host services (engine, search, each `sesh-shim`), all as launchd jobs. `up` restarts services whose config is stale. `down` stops them all. | FR-002, FR-027 |
| `sesh ls` | List repositories with sandboxes, their sessions, and state. | — |
| `sesh reset [--yes]` | Discard the current repo's sandbox and isolated clone after confirmation. Unpulled work is listed first. | FR-025 |
| `sesh logs [engine\|search\|SHIM]` | Tail a host service's log. | — |

## Output rules

- Human-readable by default. `--json` on `status`, `ls`, `policy show`
  and `verify`.
- Secret values are never printed, including in `--verbose` and in error
  messages. Placeholders may be shown.
