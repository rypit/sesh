# Data model: Sandboxed agent sessions for any terminal harness

Entities from spec.md (Key Entities), with fields, validation and state.
File formats are in [contracts/config.md](./contracts/config.md) and
[contracts/recipe.md](./contracts/recipe.md). No entity ever holds a
secret value; credentials are referenced by name and source only.

## Config (user config, `~/.config/sesh/config.toml`)

| Field | Type | Rules |
|---|---|---|
| `default_recipe` | recipe name | Must name an existing recipe. Set by `sesh init`. |
| `endpoints` | list of Endpoint | Names unique. |
| `credentials` | list of CredentialBinding | Names unique. |
| `deny` | list of Rule | Applied after all allows; deny wins. |
| `recipe.<name>` | RecipeOverride | `<name>` must exist. |
| `repo.<path>` | RepoScope | `<path>` absolute and canonical. |
| `risky_paths` | list of glob | Added to the built-in list (R7). |
| `snapshot.max_size` | size | Default 50 MB. |
| `snapshot.extra_exclude` | list of glob | Added to recipe exclusions. |

Unknown keys are an error. The file is user-owned; sesh writes it only
through `sesh init` and `sesh allow`, preserving comments where possible
and printing every change.

## Endpoint

| Field | Type | Rules |
|---|---|---|
| `name` | string | `[a-z0-9-]{1,32}`, unique across config and recipes (a config endpoint with a recipe endpoint's name overrides it). |
| `host` *or* `local_port` | hostname / 1–65535 | Exactly one. Hostnames only, no IPs or wildcards in v0.1. |
| `scheme` | `https` \| `http` | `http` only for `local_port`. |
| `methods`, `paths` | lists | Optional allow restriction (R3). |
| `credentials` | list of credential names | Each must exist; each credential's `hosts` must include this endpoint's host (or `localhost` for local ports). |
| `start` | argv | Local endpoints only. Must exit once the service is up (R8). |
| `start_timeout` | seconds | Default 60. |
| `health` | path | Local endpoints only; HTTP 2xx expected. |
| `process` | string | Local endpoints only; expected listener process name. |
| `required` | bool | Default `false`, except a recipe's model endpoint, which is required. |
| `source` | derived | `recipe:<name>`, `global`, or `repo:<path>`; shown by `sesh policy show`. |

## CredentialBinding

| Field | Type | Rules |
|---|---|---|
| `name` | string | Unique. |
| `mode` | `builtin` \| `header` \| `shim` | `builtin` = an sbx secret service; `header` = `sbx secret set-custom`; `shim` = a `sesh-shim` job. |
| `source` | `keychain:<service>` \| `1password:<ref>` \| `command:<argv>` | Never a literal value. |
| `hosts` | list | Required for `header` and `shim`. |
| `env` | variable name | `header` mode: the variable that holds the placeholder in the sandbox. |
| `shim` | shim name | `shim` mode: which `sesh-shim` module serves it. |

## Recipe

Defined in [contracts/recipe.md](./contracts/recipe.md). Loaded from the
binary (built-in) or `~/.config/sesh/recipes/` (user); a user recipe
with a built-in name replaces it. Validation: `workspace = "repo"` and
`attach = "terminal"` only (FR-004); every endpoint and credential it
names exists after merging with user config; import paths are under
`~` and not `~` itself.

## RecipeOverride (`recipe.<name>` in config)

Lets the user point a recipe at a different model endpoint or add
environment variables without copying the recipe:
`model_endpoint`, `extra_endpoints`, `env`.

## RepoScope (`repo.<path>` in config)

| Field | Rules |
|---|---|
| `extra_endpoints` | Endpoints granted for this repository only; `sesh allow` writes here. |
| `only` | If set, the compiled policy keeps only these endpoint names (plus the recipe's model endpoint, which can be named or removed explicitly). Narrows only (FR-018). |

## Sandbox

| Field | Rules |
|---|---|
| `id` | `sesh-<recipe>-<repo-id>`, where `<repo-id>` is the first 12 hex digits of SHA-256 of the canonical repository path. |
| `recipe`, `repo_path` | Identity; one sandbox per pair (FR-005). |
| `workspace` | Path of the isolated clone in the state dir. |
| `base` | Host `HEAD` commit the current session started from. |
| `policy_hash` | Hash of the compiled policy last applied. |

State (persisted in `state/sandboxes/<id>/state.toml`):

```text
absent --sesh--> created --(lock taken)--> running --(harness exits)--> idle
idle --sesh--> running
running --(crash: lock owner gone)--> idle   (stale lock cleared, FR-026)
any --sesh reset--> absent
```

## Session

| Field | Rules |
|---|---|
| `id` | Short, unique per repository: `YYYYMMDD-<n>`. Names `sesh/<id>/<branch>`. |
| `sandbox` | Owning sandbox. |
| `base` | As above; fixed for the session. |
| `returned` | List of commit ids already fetched to the host. |

A session continues across runs until `sesh --new` or `sesh reset`.

## ConfigSnapshot

Per session: the staging directory, the list of imported files, and the
list of excluded files with reasons (`credential_file`, `exclude_glob`,
`scan:<rule>`, `symlink_escape`, `size_cap`). Rebuilt every session;
never copied back.

## Policy (compiled)

An ordered, deterministic list of rules: `allow` (host or local port,
optional methods/paths) and `deny`, plus credential bindings. Built only
from Config and Recipes (FR-013); `policy.rs` takes no repository input.
Compilation rules are in contracts/config.md.

## ReturnSummary

Per return: branches fetched with commit counts, the carried-changes
commit (if any), risky paths touched, and uncommitted work left in the
sandbox. Printed on harness exit and by `sesh pull`.

## RefusedHostReport

Per session: refused hosts with counts, derived from the sbx policy log
(R3). Hosts and counts only.

## Invariant and result

`id`, `principle`, `where` (inside / host), `expected`, and a result of
`PASS`, `FAIL` (with observed) or `KNOWN-RISK` (with issue link). See
[contracts/invariants.md](./contracts/invariants.md).
