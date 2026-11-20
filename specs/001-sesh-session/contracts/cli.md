# Contract: `sesh` CLI

Exit codes for every command:

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | general error (including: a harness already running in this sandbox; the harness exited abnormally) |
| 2 | usage error (including: not a git repository, no commits) |
| 3 | platform or prerequisite not met (sbx missing, logged out or too old; required endpoint unreachable; local endpoint not loopback-only or held by another process) |
| 4 | security check failed (policy mismatch, invariant failure); sesh refused to proceed |

When the harness itself exits non-zero, `sesh` exits 1 after returning
work, and prints the harness's exit code.

## Commands

| Command | Behavior | Spec refs |
|---|---|---|
| `sesh [--new] [<recipe>] [-- ] [args…]` | In a git repo: pre-launch checks, take the sandbox lock, create or reuse the (recipe, repo) sandbox, build and copy the config snapshot, apply policy and credentials, run the recipe's command attached to the terminal with `args` appended. On exit: return work (as `sesh pull`), print the return summary and the refused-host list. Without `<recipe>`, uses `default_recipe`. `--new` starts a new session id. Arguments after the recipe name, or after `--`, go to the harness unchanged. | FR-001–FR-012, FR-015, FR-021, FR-023, FR-026, FR-027, US1, US2 |
| `sesh pull [--recipe R] [--session ID]` | Return work now: bundle commits since the session base inside the sandbox, fetch into `sesh/<session>/<branch>`, list the carried-changes commit separately, highlight risky paths, report uncommitted sandbox work and offer to commit it there first. | FR-011, FR-015, US2 |
| `sesh allow <host> [--global] [--method M]… [--path P]…` | Add an endpoint for `<host>` to user config (repo-scoped by default) and print what was added. | FR-021, US3 |
| `sesh verify [--recipe R] [--only ID…] [--json]` | Run the invariant suite against a real sandbox; PASS/FAIL/KNOWN-RISK table, expected vs observed for failures. | FR-022, US4 |
| `sesh policy show [--recipe R] [--json]` | Print the compiled policy for this repo's sandbox: endpoints with their source, method/path rules, deny rules, credential bindings (names and hosts only). | FR-025 |
| `sesh recipes [show <name>]` | List recipes with their source (built-in / user / override), or print one merged recipe. | FR-003, US5 |
| `sesh init` | One-time setup: check the platform, install and authenticate sbx, choose a default recipe and model endpoint, store credentials in the Keychain, write a default config if missing. Idempotent. | FR-024, US6 |
| `sesh doctor [--probe]` | Platform, sbx version and login, config and recipe validity, endpoints (reachable, loopback-only), shim jobs. `--probe` also sends one test request with a tool definition to the default recipe's model endpoint. No changes. | FR-023, FR-027 |
| `sesh ls [--json]` | Sandboxes with recipe, repository, session, state, and unreturned commit count. | — |
| `sesh reset [--recipe R] [--yes]` | Discard this repo's sandbox and isolated clone after listing unreturned work and confirming. | FR-025 |
| `sesh logs <shim>` | Tail a shim's log. | — |

`<recipe>` is resolved after subcommands, so a recipe can't be named
like a subcommand (contracts/recipe.md).

## Output rules

- Human-readable by default; `--json` where listed.
- Secret values are never printed, including in `--verbose` and in
  error messages. Placeholders and credential names may be shown.
- The refused-host list shows hosts and counts only, with a ready-to-run
  `sesh allow` line for each.
- The return summary shows: branches and commit counts, the
  carried-changes commit, risky paths touched, uncommitted work left in
  the sandbox.
