# Research: Sandboxed agent sessions for any terminal harness

Decisions behind `plan.md`. Items marked **Spike** need hands-on
confirmation on the reference Mac before the code that depends on them
is written (see tasks.md, Phase 2).

Evidence for R1–R3 and R7 was first gathered on 2026-11-19 from docs,
READMEs and issue trackers. On 2026-11-20 the Docker Sandboxes docs were
re-read from their source, the public
[docker/docs](https://github.com/docker/docs) repository
(`content/manuals/ai/sandboxes/` and `content/manuals/ai/governance/`),
against sbx 0.54.0 (released 2026-11-16); findings from that reading are
marked *(docs 0.54)*. Nothing below has yet been run on the reference
Mac.

---

## R1. Sandbox runtime → Docker Sandboxes (`sbx`)

**Decision**: use `sbx` microVMs, one sandbox per (recipe, repository)
pair (FR-005).

**Why**:
- Each sandbox is a microVM with its own kernel (Hypervisor.framework on
  macOS). An escape stays inside that VM.
- Each sandbox has its own Docker engine, so the agent can build and run
  containers without the host's Docker socket (FR-012).
- A host-side proxy enforces deny-by-default egress; raw TCP, UDP and
  ICMP are blocked at the network layer.
- Host-side credential injection: the sandbox holds a placeholder, and
  the proxy substitutes the real value on the way out.
- Method/path rules on allow/deny (`--method`, `--path`, `--query`,
  deny wins), added around v0.50.
- sbx ships agent kits for several harnesses, which recipes reuse
  (R4).
- *(docs 0.54)* Platforms: macOS 14+ on Apple Silicon (installed with
  `brew install docker/tap/sbx`), Windows 11 x64 with Windows
  Hypervisor Platform, and Ubuntu 24.04+ (x86-64 or arm64) with KVM.
  sesh's platform scope is a sesh choice, not an sbx limit (ADR 0005).

**Alternatives considered**:
- *Hardened Docker container + host mitmproxy*: shares one VM with every
  other container, gives the agent no safe Docker, and leaves us to build
  egress control, injection and DNS lockdown. Fallback only.
- *Anthropic `sandbox-runtime` (`srt`)*: Seatbelt, no VM; shared kernel
  and no safe Docker for the agent.
- *microsandbox*: open-source microVMs with host-scoped secrets; beta.
- *Strands Box*: rich policy model, preview, appears to use Seatbelt.

**Constraints to carry**:
- Pin a minimum sbx version that has method/path rules.
- `sbx policy check` and `sbx policy log` don't evaluate method/path, so
  invariants must test them directly.

## R2. Credentials → sbx injection, plus `sesh-shim` for the gaps

**Decision**:
- Built-in sbx secret services where they exist (e.g. `github`).
- Other header-based APIs, including model providers' API keys, use
  `sbx secret set-custom --host … --env …`, with the value pulled at
  need through `--command` (Keychain via `security
  find-generic-password`) or `--ref` (1Password). The harness reads the
  placeholder from the environment variable it already expects
  (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, …).
- Anything the proxy can't handle goes through a shim (FR-020, ADR 0003).

**Known gaps in sbx custom secrets** (experimental since v0.27):
- Substitution happens in **any** header of a request to a bound host
  ([docker/sbx-releases#707](https://github.com/docker/sbx-releases/issues/707)).
  Mitigations: unique placeholders, never bind secrets to hosts with
  echo endpoints, path deny rules, and an echo-server invariant
  (INV-S3, KNOWN-RISK while open).
- Headers only: no substitution in query strings or bodies.
- Base64-encoded credentials (HTTP Basic) don't match the placeholder
  ([docker/sbx-releases#7](https://github.com/docker/sbx-releases/issues/7)).
- No request signing (SigV4, HMAC).

**Shim design** (ADR 0003): `sesh-shim <name>` under its own launchd
job, bound to `127.0.0.1`, reading its secret from the Keychain per
request, forwarding only to its one upstream on allowed routes.

**Built-in secret services** *(docs 0.54)*: `sbx secret set <service>`
for anthropic, cursor, droid, github, google, groq, mistral, nebius,
openai, openrouter and xai; the proxy injects the key on requests to
that provider's API domains. sbx stores them in the OS keychain (a
permission-protected file on headless Linux). Built-in recipes use these
services (`mode = "builtin"`) rather than `set-custom` wherever one
exists. `set-custom` is still marked experimental; its form is
`sbx secret set-custom --host H --env VAR (--value-stdin | --command … |
--ref …) [--sandbox S]`, and it substitutes in HTTP headers only.

**Subscription logins** *(docs 0.54)*: "OAuth runs on the host and the
token never enters the sandbox." Claude Code's `/login` inside the
sandbox works for Max, Team and Enterprise plans (Pro is not mentioned);
Codex uses `sbx secret set openai --oauth`, with the prompt shown on the
host. This removes the main risk to US1's "already logged in" scenario
for the `claude` and `codex` recipes. **Spike** (T008) narrows to:
confirm both flows, check Claude Pro, and find whether opencode and pi
can use these logins or need API keys. A login that can't be attached
this way is still refused, not copied in (FR-020).

**Token hygiene**: fine-grained GitHub PAT limited to the repos being
worked on, no gist or admin scopes; branch protection on `main`.

## R3. Network policy shape and refused-host reporting

- Local endpoints are allowed as `localhost:<port>` and reached from
  inside the sandbox as `host.docker.internal:<port>` *(docs 0.54:
  `sbx policy allow network localhost:11434`, then
  `curl http://host.docker.internal:11434`)*. A rule written for
  `host.docker.internal:<port>` reportedly shows "Allowed" in
  `sbx policy check` but still returns 403; **Spike** (T007) confirms
  only that other host ports and LAN addresses stay refused.
- Rules are scoped per sandbox with `--sandbox <name>`, or at creation
  with `sbx create --deny-network …` *(docs 0.54)*. HTTP rule form:
  `sbx policy allow network <host> --method GET,POST --path '/repos/**'
  [--query k=v]`; methods comma-separated or `ANY`, paths start with
  `/`.
- Baseline: start from sbx's strictest preset; add only what the
  compiled policy needs. Preset names: **Spike** (T005).
- HTTP rules match hostnames only; IP or CIDR destinations need plain
  network rules.

**Refused hosts (FR-021)**: `sbx policy log` records proxy decisions
(see R1 constraints); the docs read so far mention only the dashboard's
"connection logs", and the CLI reference is probably in
`governance/local/inspect.md`, not yet read. sesh reads it for the sandbox after the harness
exits and groups denials by host, keeping only hosts and counts (never
paths, headers or bodies, which can carry data). Its output format and
whether it can be filtered by sandbox and time: **Spike** (T005).
Non-HTTP denials (raw TCP/UDP, DNS) may not appear there; if not, the
report says so and lists only HTTP(S) hosts.

## R4. Recipes → TOML data, sbx kits for installation

**sbx agent kits** *(docs 0.54)*: `sbx run <agent>` for `claude`,
`codex`, `opencode`, `copilot`, `cursor`, `kiro`, `droid`, `devin`,
`docker-agent` and `shell` (`gemini` retired in 0.53). Arguments after
`--` go to the agent (`sbx run claude -- --model sonnet`); if the first
starts with `-`, the kit's defaults are kept and the arguments added.
Three of the four built-in recipes (`claude`, `codex`, `opencode`) can
use a kit directly. **pi has no kit**: its recipe uses a custom kit or
an OCI image built on the `shell` agent.

**Decision**: a recipe is a TOML file (contracts/recipe.md). Built-in
recipes are embedded in the binary; user recipes live in
`~/.config/sesh/recipes/<name>.toml`, and a user recipe with a built-in
name overrides it (FR-003). No recipe is ever read from a repository
(Principle V).

A recipe declares:
- **install**: an sbx agent kit name where sbx ships one, otherwise a
  custom kit or an OCI image;
- **launch**: argv and working directory;
- **import**: host config paths to snapshot, exclusions, and the files
  that hold credentials;
- **endpoints**: the endpoints and credential bindings the harness needs
  by default, and which one is the model endpoint (overridable in user
  config, so the same recipe can point at a cloud API or a local
  server);
- **env / files**: how the harness learns endpoint locations, as
  environment variables or small generated config files, with only
  `{endpoint.<name>.url}`-style substitution (no template language).

**Why data, not code**: adding or fixing a harness doesn't need a sesh
release, and a recipe can be reviewed in a minute. See ADR 0004 for the
rejected adapter-in-code alternative.

**Built-in recipe facts to confirm** (from memory of each harness's
current layout; each recipe is validated in its spike before it ships):

| Recipe | Config to import | Credential files (excluded) | Model endpoint wiring |
|---|---|---|---|
| `claude` (Claude Code) | `~/.claude/` (settings, `CLAUDE.md`, agents, commands, skills, plugins), the MCP and preferences parts of `~/.claude.json` | `~/.claude/.credentials.json`; OAuth state in `~/.claude.json`; on macOS the login is in the Keychain and so never copied | `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY` placeholder |
| `codex` (Codex CLI) | `~/.codex/config.toml`, `~/.codex/AGENTS.md`, prompts | `~/.codex/auth.json` | `OPENAI_BASE_URL`, `OPENAI_API_KEY` placeholder, or a provider entry in a generated `config.toml` |
| `opencode` | `~/.config/opencode/` | `~/.local/share/opencode/auth.json` | provider `baseURL` in a generated `opencode.json` overlay |
| `pi` | `~/.pi/agent/` (settings, extensions, skills) | `~/.pi/agent/auth.json`; `apiKey` fields in `models.json` | provider `baseUrl` in a generated `models.json` |

**Spike** (T009): for each built-in recipe, confirm the kit or image,
config and credential paths, and endpoint wiring, and record any
mismatch here. `~/.claude.json` mixes preferences with account state, so
the `claude` recipe needs a field-level filter rather than a whole-file
copy; whether that's worth it or the file is simply excluded is decided
in the spike.

**Telemetry and update hosts**: built-in recipes declare only the hosts
the harness needs to work. Telemetry, error-reporting and update-check
hosts stay blocked and show up in the refused-host list; the user can
`sesh allow` them.

## R5. Config snapshot → host-side staging, copied in per session

**Decision**: at session start sesh builds the snapshot in a staging
directory under its state dir (`~/.local/state/sesh/snapshots/<sandbox>/`),
then copies it into the sandbox user's home, replacing the previous
copy. The sandbox never sees the host originals or the staging dir, so
nothing it writes can reach them (FR-007).

Rules applied while staging:
- **No symlink escapes**: a symlink whose target leaves the declared
  path is skipped and reported (a link from `~/.claude/` to `~/.ssh/`
  must not import SSH keys). Links inside the path are copied as files.
- **Exclusions first**: recipe `credential_files` and `exclude` globs
  are removed before scanning.
- **Scan** (R6), then drop matches.
- **Size cap**: a snapshot over a configurable size (default 50 MB) is
  refused with a message naming the largest entries, so a stray cache
  isn't copied every session.
- Regular files keep their mode bits except setuid/setgid.

**Copy mechanism** *(docs 0.54)*: `sbx cp <src> <sandbox>:<abs-path>`
copies host files into a sandbox, and `sbx run` accepts extra mounts
with a `:ro` suffix. Decision: copy with `sbx cp` after each session
start (a copy can't be written back by construction). A read-only mount
of the staging dir is the fallback if copying is too slow, acceptable
only if the invariants show the sandbox can't write through it.
**Spike** (T010): time `sbx cp` for a typical `~/.claude/`.

## R6. Credential scanning → small built-in rule set

**Decision**: a fixed set of high-confidence rules, compiled into sesh,
applied to every text file in the snapshot (FR-008):

- known token prefixes: `sk-ant-`, `sk-proj-`, `sk-` followed by 40+
  base62 characters, `ghp_`, `gho_`, `github_pat_`, `glpat-`,
  `xoxb-`/`xoxp-`, `AKIA`/`ASIA` + 16 characters, `AIza` + 35;
- PEM private key headers (`-----BEGIN … PRIVATE KEY-----`);
- JSON/TOML/YAML keys named like `api_key`, `apiKey`, `token`,
  `access_token`, `refresh_token`, `secret`, `password` with a non-empty
  string value of 16+ characters that isn't an environment-variable
  reference (`$FOO`, `${FOO}`, `{env:FOO}`).

A matching file is excluded whole and reported by path and rule name,
never by value. Users can add exclusions in config but cannot disable
the rule set.

**Why built-in**: an external scanner (gitleaks, trufflehog) would be a
second required install and a second supply-chain dependency for a
check that needs only a few dozen rules. This needs the `regex` crate,
justified in plan.md.

**Accepted limit**: a credential in an unrecognized format can slip
through. The recipe's `credential_files` list is the primary control;
the scan is a backstop. INV-S6 plants one value per rule.

## R7. Workspace and returning work

**Problem**: if the sandbox's workspace is the user's checkout, the agent
can write `.git/config` and `.git/hooks`, and ordinary host git use runs
agent-authored commands (`core.fsmonitor`, `core.hooksPath`,
`filter.*`, `diff.external`, `core.pager`). Plus non-git vectors.

**Decision**: the workspace is an **isolated clone** in sesh's state
directory, never the checkout itself (ADR 0001):
- The host checkout's `.git` is never mounted (FR-014).
- Carried changes become one commit with a `Sesh-Carried-Changes: yes`
  trailer (FR-011); its parent is the host `HEAD` at session start,
  recorded as the session's base.
- On harness exit sesh runs `git bundle create` *inside* the sandbox for
  `<base>..` on every branch, then the host runs `git fetch <bundle>
  'refs/heads/*:refs/heads/sesh/<session>/*'` (FR-015). A bundle is pure
  data; host git never reads the sandbox repo's config.
- Returned commit ids are recorded in
  `state/sessions/<sandbox>/returned`, for `sesh reset`.

**Risky-path list** (highlighted in the return summary, US2-2), matched
against files changed in returned commits:
- environment and shell: `.envrc`, `.env*`, `.tool-versions`,
  `.mise.toml`, `.nvmrc`-style files only if paired with auto-switch
  tools;
- editors: `.vscode/**`, `.idea/**`, `*.code-workspace`, `.zed/**`;
- agent configs that a host harness would load: `.claude/**`,
  `.codex/**`, `.opencode/**`, `.pi/**`, `AGENTS.md`, `CLAUDE.md`
  (project hooks run when the user next runs that harness on the host);
- build and task runners: `Makefile`, `*.mk`, `justfile`, `Taskfile*`,
  `build.rs`, `.cargo/config*`, `setup.py`, `pyproject.toml`,
  `package.json` (scripts), `.npmrc`, `.yarnrc*`;
- git and CI: `.gitattributes`, `.gitmodules`, `.husky/**`,
  `.pre-commit-config.yaml`, `.github/workflows/**`, `.gitlab-ci.yml`.

The list is data in sesh, extendable in user config.

**Workspace exposure** *(docs 0.54)*: by default `sbx run` mounts the
current directory **read-write** at the same absolute path; another
directory can be given (`sbx run claude ~/my-project`). `--clone` gives
the agent its own Git clone inside the sandbox and mounts the host repo
**read-only** at `/run/sandbox/source`.

**Decision: sesh keeps its own isolated clone and does not use
`--clone`** (ADR 0005). `sesh` passes the isolated clone's directory
as the workspace, so the host checkout is never mounted at all. Why:
`--clone` still exposes the whole host checkout to the agent, read-only:
ignored files such as `.env` with real secrets (Principle III), and
`.git/config`, which can hold tokens in remote URLs or credential-helper
settings. sesh's clone contains only tracked files plus untracked,
non-ignored ones (FR-011), and its remotes are rewritten to drop
embedded credentials. How `--clone` returns work isn't documented;
sesh's bundle return (FR-015) doesn't depend on it.

**Spike** (T010): confirm the host checkout is unreachable when the
isolated clone is the workspace, and time bundle export and import on a
10,000-file repository against SC-006.

## R8. Endpoints → declared, checked, optionally started

**Decision**: an endpoint is a host (with optional method/path rules) or
a local port, plus optional credential bindings, an optional start
command and a `required` flag (contracts/config.md).

- **Local endpoints** must listen on `127.0.0.1`/`::1` only, and the
  listening process must match the endpoint's optional `process` name.
  sesh checks with `lsof -nP -iTCP:<port> -sTCP:LISTEN` behind a seam
  (FR-027). sesh doesn't authenticate the service beyond that; users
  who want more can put a token on it and bind a credential.
- **Start commands** must return once the service is started (e.g.
  `brew services start ollama`, `sovereign up`). sesh runs the command,
  waits up to `start_timeout` (default 60 s) for the endpoint to answer,
  and kills the command if it hasn't exited by then. sesh never stays
  the parent of a long-running service (no sesh daemon).
- **Health**: a TCP connect for hosts; for local endpoints with
  `health = "<path>"`, an HTTP GET that must return 2xx.
- **Model endpoint probe** (`sesh doctor --probe`): one tiny request
  with a tool definition to the recipe's model endpoint, to catch
  engines whose tool calling doesn't work before a session starts.
  Optional, because it costs a request on paid APIs.

## R9. Implementation language → Rust

**Decision** (ADR 0002, amended by ADR 0003): Rust, stable, edition
2024; one Cargo package with binaries `sesh` and `sesh-shim`; `unsafe`
forbidden. Crates: `clap`, `serde` + `toml`, `anyhow`, `ureq` (health
checks), `regex` (R6), `globset` (recipe exclusions and the risky-path
list). Shim HTTP/TLS crates are justified when the first shim lands.

**Why**: compiler-checked error handling and exhaustive matching suit
agent-written code; `sesh-shim` is a long-running process holding real
credentials; single binaries simplify installation.

## R10. SDD tooling → Spec Kit

Spec Kit v1.4.2 is initialized (`specify init --here`, Claude
integration, `sh` scripts); the constitution was kept.

## R11. Lessons carried from Sovereign

Keep: strict config (`deny_unknown_fields`), one seam per external CLI
with fakes in tests, ADRs for contract changes, launchd for long-running
host processes (now only shims). Drop: multi-engine routing, memory
admission, telemetry, dashboards, benchmarks. Sovereign itself can serve
as a user's local inference endpoint (ADR 0004).

## R12. Prior art: n8n Nod

[n8n-io/nod](https://github.com/n8n-io/nod) (Apache-2.0, "research
preview"; read from its README on 2026-11-20; its security-model docs
were not reachable) is an open-source personal agent that does errands
on real websites and apps, through a desktop app, Telegram or WhatsApp.

- **Shape**: a Node server plus Docker containers on the user's machine
  (Linux workspace, Chromium, PostgreSQL), using the **pi** agent
  runtime. Containers, not a microVM: they share a kernel, and the
  README describes no egress allowlist.
- **Keys out of the sandbox**: "Provider keys never enter the workspace
  or the browser"; a Vault fills passwords and cards on websites.
  Matches Principle III.
- **Policy in the server**: "Policy lives in the server, not the
  prompt." Matches Principle II in spirit.
- **Untrusted content as data**: app, web and browser results reach the
  model "marked as data, not instructions"; memory learns only from the
  user's own messages, never from pages it reads.
- **Approvals by model judgement**: actions not covered by the request
  and rules wait for the user's yes, but the check is a model's
  judgement; its own test reports one model letting 8 unsafe actions
  through out of 240 decisions.
- **Telemetry on by default**, and app connections go through an
  n8n-hosted service by default; both conflict with sesh's constitution.

**Relevance**: not feature 001 (Nod isn't a terminal coding harness).
Useful for the future always-on agent feature: its approval flow,
"results as data", and user-only memory are worth adopting there, on top
of sesh's deterministic, verifiable boundary rather than instead of it.

## Superseded (ADR 0004)

The first spec's decisions to run Splash (oMLX fallback) and SearXNG on
the host and to launch pi through a custom kit are withdrawn: sesh no
longer starts inference or search, and pi is one recipe among several.
Facts worth keeping for users who choose these as endpoints:
- Splash serves OpenAI and Anthropic APIs on loopback with no auth by
  default, and does not schema-validate tool-call arguments.
- SearXNG with JSON output works as a search endpoint; queries leave the
  machine through its upstream engines.

---

## Sources

- Docker Sandboxes: [security model](https://docs.docker.com/ai/sandboxes/security/),
  [isolation](https://docs.docker.com/ai/sandboxes/security/isolation/),
  [credentials](https://docs.docker.com/ai/sandboxes/configuration/credentials/),
  [`secret set-custom`](https://docs.docker.com/reference/cli/sbx/secret/set-custom/),
  [local policy](https://docs.docker.com/ai/sandboxes/security/policy/),
  [develop locally / host services](https://docs.docker.com/ai/sandboxes/workflows/development/),
  [upstream proxy](https://docs.docker.com/ai/sandboxes/configuration/upstream-proxy/),
  [build an agent kit](https://docs.docker.com/ai/sandboxes/customize/build-an-agent/),
  [kits v2](https://docs.docker.com/ai/sandboxes/customize/kits-v2/),
  [release notes](https://docs.docker.com/ai/sandboxes/release-notes/)
- sbx issues: [#7](https://github.com/docker/sbx-releases/issues/7),
  [#239](https://github.com/docker/sbx-releases/issues/239),
  [#707](https://github.com/docker/sbx-releases/issues/707)
- [varlock: Docker Sandboxes](https://varlock.dev/sandboxes/docker-sandboxes/)
- Alternatives: [anthropics/sandbox-runtime](https://github.com/anthropics/sandbox-runtime),
  [microsandbox](https://github.com/superradcompany/microsandbox),
  [Strands Box](https://github.com/strands-agents/box),
  [Infisical Agent Vault](https://github.com/Infisical/agent-vault)
- Prior art: [n8n-io/nod](https://github.com/n8n-io/nod)
- Superseded: [incoai/splash](https://github.com/incoai/splash),
  [pi](https://pi.dev/)
- [GitHub Spec Kit](https://github.github.com/spec-kit/)
