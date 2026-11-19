# Contract: `~/.config/sesh/config.toml`

The only source of policy (Principle V). Every key is optional: an
absent file means "all defaults". Unknown keys are an error
(`extra="forbid"`). The file never contains secret values, only
references to where they live.

```toml
# Defaults shown. Port numbers are placeholders until spike T006 settles
# Splash's default port.

[engine]
kind  = "splash"                                  # "splash" | "omlx" (later)
model = "unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_M"
port  = 8800                                       # loopback only
# api_key = "keychain:sesh/splash"                # optional; injected by sbx

[search]
enabled = true
port    = 8888                                     # SearXNG, loopback only

[session]
profile = "online"                                 # "online" | "offline"
harness = "pi"                                     # "pi" | "claude" (fallback)

# Package registries the sandbox may reach in the online profile.
[registries]
allow = ["registry.npmjs.org", "pypi.org", "files.pythonhosted.org", "crates.io", "static.crates.io"]

# Documentation hosts for the online profile (R7).
[docs]
allow = ["docs.python.org", "developer.mozilla.org", "raw.githubusercontent.com"]

# --- Credentials -----------------------------------------------------------
# source: "keychain:<service>" | "1password:<op://…>" | "command:<argv>"
# mode:   "builtin" (an sbx service) | "header" (sbx set-custom) | "shim"

[[credentials]]
name   = "github"
mode   = "builtin"                                 # sbx `secret set github`
source = "keychain:sesh/github"                    # fine-grained PAT, no gist/admin scopes

[[credentials]]
name   = "brave"
mode   = "header"
hosts  = ["api.search.brave.com"]
env    = "BRAVE_API_KEY"                           # holds a placeholder inside the sandbox
source = "keychain:sesh/brave"

# --- Extra API rules ---------------------------------------------------------
[[allow]]
host    = "api.github.com"
methods = ["GET", "POST", "PATCH"]

[[deny]]
host    = "api.github.com"
methods = ["DELETE"]
paths   = ["/repos/**"]

# --- Tools (FR-020) ----------------------------------------------------------
# Built-in search is declared internally the same way.
# [[tools]]
# name        = "weather"
# service     = { command = ["sesh-shim-weather"], port = 8901 }   # host side, loopback
# credentials = ["weather"]                                         # used by the shim, never the sandbox
# harness     = { pi_extension = "~/.config/sesh/tools/weather.ts" }
```

## Compilation rules (`sandbox/policy.py`)

1. Start from sbx's strictest baseline.
2. Always allow `localhost:<engine.port>`.
3. Online profile only:
   - `localhost:<search.port>` if search is enabled,
   - each registry and docs host,
   - each credential's hosts,
   - each `[[allow]]` rule,
   - each tool's `localhost:<port>`.
4. Add every `[[deny]]` rule. Deny wins.
5. The offline profile is step 1 plus step 2 only.
6. Output is a sorted, deterministic rule list, so `policy show` and the
   applied-vs-expected check (FR-022) compare exactly.
