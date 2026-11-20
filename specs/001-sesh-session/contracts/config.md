# Contract: `~/.config/sesh/config.toml`

With recipes (contracts/recipe.md), the only source of policy
(Principle V). Every key is optional: an absent file means "defaults",
which is enough for built-in recipes once their credentials are in the
Keychain. Unknown keys are an error (`deny_unknown_fields`). The file
never contains secret values, only references to where they live.

```toml
default_recipe = "claude"                      # written by `sesh init`

# --- Endpoints (FR-016) -----------------------------------------------------
# Package registries most projects need. `sesh init` writes these.
[[endpoints]]
name = "npm"
host = "registry.npmjs.org"

[[endpoints]]
name = "pypi"
host = "pypi.org"

[[endpoints]]
name = "pypi-files"
host = "files.pythonhosted.org"

[[endpoints]]
name = "crates"
host = "index.crates.io"

[[endpoints]]
name = "github"
host = "api.github.com"
methods = ["GET", "POST", "PATCH"]
credentials = ["github"]

# A local inference server the user runs (Splash, Ollama, LM Studio,
# Sovereign…). sesh only checks, starts (optionally) and allows it.
[[endpoints]]
name          = "local-llm"
local_port    = 11434
api           = "openai"
health        = "/v1/models"
process       = "ollama"                       # optional listener check
start         = ["brew", "services", "start", "ollama"]   # must exit once started
start_timeout = 60

# --- Credentials (FR-019, FR-020) --------------------------------------------
# source: "keychain:<service>" | "1password:<op://…>" | "command:<argv>"
# mode:   "builtin" (an sbx service) | "header" (sbx set-custom) | "shim"

[[credentials]]
name    = "github"
mode    = "builtin"                            # stored by sbx: `sbx secret set github`
service = "github"                             # fine-grained PAT, no gist/admin scopes
# `sesh init` runs `sbx secret set <service>`; sbx keeps the value in the
# OS keychain. header/shim credentials instead name a `source`.

# --- Deny rules (deny wins) ----------------------------------------------------
[[deny]]
host    = "api.github.com"
methods = ["DELETE"]
paths   = ["/repos/**"]

# --- Recipe overrides ------------------------------------------------------------
# Point Claude Code at the local server instead of Anthropic's API.
# [recipe.claude]
# model_endpoint = "local-llm"

# --- Per-repository scope (FR-018, FR-021) ------------------------------------
# `sesh allow <host>` adds to extra_endpoints of the current repository.
# [repo."/Users/me/code/secret-project"]
# only            = ["local-llm"]               # narrows: nothing else, not even registries
# extra_endpoints = [{ name = "docs-rs", host = "docs.rs" }]

# --- Return summary and snapshot ---------------------------------------------
# risky_paths = ["deploy/**"]                   # added to the built-in list
# [snapshot]
# max_size      = "50MB"
# extra_exclude = [".claude/local-notes/**"]
```

## Compilation rules (`sandbox/policy.rs`)

Inputs: this config, the selected recipe (with its override), and the
current repository's path, used only as a key into `repo.<path>`. No
repository content is read (FR-013).

1. Start from sbx's strictest baseline.
2. Collect endpoints: the recipe's endpoints, then global `endpoints`,
   then the repository's `extra_endpoints`. A later endpoint with the
   same name replaces an earlier one.
3. If the repository has `only`, keep only those names.
4. Turn each endpoint into allow rules: `host` (with methods/paths) or
   `localhost:<local_port>`.
5. Add every `deny` rule. Deny wins.
6. Attach each endpoint's credential bindings; a binding whose `hosts`
   don't cover its endpoint is a config error.
7. Sort rules deterministically; the result's hash is recorded on the
   sandbox and compared with sbx's read-back before every session
   (FR-023).

## `sesh allow` writes

`sesh allow <host> [--global] [--method M]… [--path P]…` appends one
endpoint named after the host (dots replaced by dashes) to
`repo."<path>".extra_endpoints`, or to the global `endpoints` with
`--global`, and prints the TOML it added. It never adds credentials.
