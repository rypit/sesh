# Contract: recipe format

A recipe is a TOML file describing one harness. Built-in recipes are
embedded in sesh; user recipes live in `~/.config/sesh/recipes/<name>.toml`
and override a built-in recipe of the same name. Recipes are never read
from a repository (Principle V). Unknown keys are an error. A recipe
never contains a secret value.

Recipe names must not collide with sesh subcommands (`pull`, `verify`,
`policy`, `allow`, `init`, `doctor`, `ls`, `reset`, `recipes`, `logs`,
`help`).

## Example: built-in `claude`

Paths and variable names are confirmed per recipe in spike T009
(research.md R4) before the recipe ships.

```toml
name        = "claude"
description = "Claude Code"
workspace   = "repo"        # only "repo" in this feature (FR-004)
attach      = "terminal"    # only "terminal" in this feature

[install]
kit = "claude"              # an sbx agent kit; or: image = "<oci ref>"

[launch]
command = ["claude"]        # user args are appended unchanged (FR-001)
workdir = "{workspace}"

[import]
# host path -> path in the sandbox user's home
paths = [
  { host = "~/.claude", sandbox = "~/.claude" },
]
credential_files = [".claude/.credentials.json"]
exclude          = [".claude/projects/**", ".claude/statsig/**", ".claude/todos/**"]

# Endpoints the harness needs by default. User config can override any
# of them by name, or point the recipe at another model endpoint.
model_endpoint = "anthropic"

[[endpoints]]
name        = "anthropic"
host        = "api.anthropic.com"
credentials = ["anthropic"]

[[credentials]]
name   = "anthropic"
mode   = "header"
hosts  = ["api.anthropic.com"]
env    = "ANTHROPIC_API_KEY"
source = "keychain:sesh/anthropic"

# How the harness learns where its model endpoint is.
[env]
ANTHROPIC_BASE_URL = "{model_endpoint.url}"
```

## Example: built-in `pi`, with a generated file

```toml
name           = "pi"
description    = "pi coding agent"
workspace      = "repo"
attach         = "terminal"
model_endpoint = "anthropic"

[install]
image = "ghcr.io/…/sesh-pi:<version>"   # or a custom kit; spike T009

[launch]
command = ["pi"]

[import]
paths            = [{ host = "~/.pi/agent", sandbox = "~/.pi/agent" }]
credential_files = [".pi/agent/auth.json", ".pi/agent/models.json"]

# Written into the sandbox after the snapshot, replacing any imported copy.
[[files]]
path    = "~/.pi/agent/models.json"
content = '''
{ "providers": { "sesh": { "baseUrl": "{model_endpoint.url}",
  "api": "{model_endpoint.api}", "apiKey": "{model_endpoint.env}" } } }
'''
```

## Fields

| Field | Required | Meaning |
|---|---|---|
| `name`, `description` | yes | `name` matches the file name. |
| `workspace`, `attach` | no | Defaults `repo`, `terminal`; other values are refused (FR-004). |
| `install.kit` \| `install.image` | one | How sbx gets the harness into the sandbox. |
| `launch.command` | yes | argv; `{workspace}` is the only substitution. |
| `launch.workdir` | no | Default `{workspace}`. |
| `import.paths` | no | Host paths under `~`, copied per session (FR-007). |
| `import.credential_files` | no | Relative to `~`; never copied (FR-008). |
| `import.exclude` | no | Globs relative to `~`; never copied. |
| `model_endpoint` | yes | Name of the endpoint the harness uses for its model. |
| `endpoints`, `credentials` | no | Same schema as in contracts/config.md. |
| `env` | no | Variables set for the harness. Values may use substitutions. |
| `files` | no | Small files written into the sandbox after the snapshot. |
| `api` | no | On an endpoint: `anthropic` \| `openai` \| `openai-responses`, used by `{…api}` and `sesh doctor --probe`. |

**Substitutions** (plain text replacement, no logic):
`{workspace}`, `{model_endpoint.url}`, `{model_endpoint.api}`,
`{model_endpoint.env}` (the credential's placeholder variable name),
`{endpoint.<name>.url}`. A local endpoint's URL is
`http://host.docker.internal:<port>`. An unknown substitution is an
error when the recipe loads.

## Rules

- A recipe may add endpoints and credential bindings; they appear in
  `sesh policy show` with source `recipe:<name>`.
- A recipe can't relax deny rules, read files outside its `import.paths`,
  or run anything on the host.
- `sesh recipes` lists available recipes and their source;
  `sesh recipes show <name>` prints the merged recipe after overrides.
