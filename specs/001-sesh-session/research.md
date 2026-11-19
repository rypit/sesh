# Research: One-command protected agent session

Decisions behind `plan.md`, with the evidence gathered so far. Items
marked **Spike** need hands-on confirmation on the reference Mac before
the code that depends on them is written (tasks T005–T011).

Evidence was gathered 2026-11-19 from docs, READMEs and issue trackers;
nothing below has yet been run on the reference Mac.

---

## R1. Sandbox runtime → Docker Sandboxes (`sbx`)

**Decision**: use `sbx` microVMs, one sandbox per repository.

**Why**:
- Each sandbox is a microVM with its own kernel (Hypervisor.framework on
  macOS). An escape stays inside that VM.
- Each sandbox has its own Docker engine, so the agent can build and run
  containers without the host's Docker socket (FR-008).
- A host-side proxy enforces deny-by-default egress; raw TCP, UDP and
  ICMP are blocked at the network layer.
- Host-side credential injection: the sandbox holds a placeholder, and
  the proxy substitutes the real value on the way out.
- Method/path rules on allow/deny (`--method`, `--path`, `--query`,
  deny wins), added around v0.50.

**Alternatives considered**:
- *Hardened Docker container + host mitmproxy*: protects the macOS host
  almost as well against realistic prompt-injection attacks (all
  containers already run inside Docker's Linux VM), but shares one VM
  with every other container, gives the agent no safe Docker, and leaves
  us to build egress control, injection and DNS lockdown. Kept as the
  fallback if sbx's experimental features become a problem.
- *Anthropic `sandbox-runtime` (`srt`)*: lowest overhead (Seatbelt,
  no VM) and open source, with credential masking scoped by
  `injectHosts`. Rejected for now: shared kernel, and no safe Docker for
  the agent.
- *microsandbox*: open-source microVMs with host-scoped secrets and a
  per-header allowlist for substitution. Still beta; a candidate if sbx
  falls short.
- *Strands Box*: richest policy model (method/path, MCP tool arguments,
  SigV4), but in preview and appears to use Seatbelt rather than a VM.

**Constraints to carry**:
- Pin a minimum sbx version that has method/path rules.
- `sbx policy check` and `sbx policy log` don't evaluate method/path, so
  invariants must test them directly.

## R2. Credentials → sbx injection, plus host shims for the gaps

**Decision**:
- GitHub uses sbx's built-in `github` secret service.
- Other header-based APIs use `sbx secret set-custom --host … --env …`,
  with the value pulled at need through `--command` (reading the
  Keychain via `security find-generic-password`) or `--ref` (1Password).
- APIs the proxy can't handle go through a host-side shim (FR-016).

**Known gaps in sbx custom secrets** (experimental since v0.27, visible
since v0.32):
- Substitution happens in **any** header of a request to a bound host
  ([docker/sbx-releases#707](https://github.com/docker/sbx-releases/issues/707),
  open). If that host echoes headers back, the agent can read the real
  value. Mitigations: unique placeholders, never bind secrets to hosts
  with echo endpoints, path deny rules, and an invariant using a local
  echo server.
- Headers only: no substitution in query strings or bodies.
- Base64-encoded credentials (HTTP Basic) don't match the placeholder
  ([docker/sbx-releases#7](https://github.com/docker/sbx-releases/issues/7)).
  This breaks git over HTTPS to hosts other than GitHub.
- No request signing (SigV4, HMAC).

**Shim design**: one small host service per awkward API. It binds to
loopback, reads its secret from the Keychain per request, adds the auth
the upstream needs, and forwards only to its one upstream and only on
allowed routes. The sandbox's policy allows only `localhost:<shim-port>`.
No proxy chain and no extra CA inside the sandbox.

**Later option**: sbx can forward sandbox traffic to an upstream proxy
(`proxy.sandbox` / `DOCKER_SANDBOXES_PROXY`). A mitmproxy upstream could
add approval prompts and one audit log, but whether sbx's proxy trusts
an intercepting upstream's CA is undocumented. Deferred.

**Token hygiene**: fine-grained GitHub PAT, limited to the repos being
worked on, with no gist or admin permissions; branch protection on
`main`.

## R3. Network policy shape

- Host services are allowed as `localhost:<port>`, and reached from
  inside the sandbox as `host.docker.internal:<port>`. A rule written for
  `host.docker.internal:<port>` reportedly shows "Allowed" in
  `sbx policy check` but still returns 403. **Spike** (T007).
- Rules can be scoped per sandbox with `--sandbox <name>`.
- Baseline: start from sbx's strictest preset and add only what the
  compiled policy needs. The preset names (allow-all / balanced /
  deny-all) come from third-party write-ups. **Spike** (T005).
- HTTP rules match hostnames only; IP or CIDR destinations need plain
  network rules.

## R4. Inference engine → Splash (oMLX fallback)

**Decision**: Splash ([incoai/splash](https://github.com/incoai/splash))
under a launchd user agent, bound to loopback.

- Vendor-reported speed on M5 Pro: Qwen3.8-27B about 74 tok/s (vs oMLX
  about 38) on short prompts, about 54 tok/s at 32K. The 74 tok/s needs a
  non-default reasoning effort.
- Serves OpenAI (Chat Completions, Responses, Completions) and Anthropic
  Messages APIs, with streaming and tool calls.
- **Tool-call arguments are not schema-validated** (1.4–1.5 release
  notes). The harness side must validate them.
- Requires M3 or later, macOS 26.4+, at least 36 GB of memory.
- Install: `brew install incoai/tap/splash`, then
  `splash serve --model unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_M`.
- **No auth by default** on localhost. Optionally set `SPLASH_API_KEY`
  and have sbx inject it on requests to the engine endpoint. Whether sbx
  can bind a secret to a `localhost` destination: **Spike** (T008).
- Splash's default port and its launchd-friendly flags: **Spike** (T006).

**Fallback**: oMLX, for models outside Splash's narrow set. It sits
behind the same engine seam.

## R5. Harness → pi

**Decision**: pi (`@earendil-works/pi-coding-agent`, renamed from
`@mariozechner/pi-coding-agent`), run inside the sandbox.

- Custom providers via `~/.pi/agent/models.json`, with API types
  `openai-completions`, `openai-responses`, `anthropic-messages` and
  `google`. Local servers are configured with a `baseUrl`.
- Extensions are TypeScript modules that can add tools, commands and UI.
- **No built-in MCP**; MCP support would come from an extension.
  sesh's tools (search, and custom tools later) are delivered as pi
  extensions (FR-020).
- Packaging inside the sandbox: a custom sbx kit. The kit format is
  unsettled: the "Build your own agent kit" tutorial uses a `spec.yaml`
  (`schemaVersion: "1"`), while newer docs describe v2 and a v3
  buildx-based descriptor (`# syntax=docker/sandbox-kit:3`).
  **Spike** (T009).

**Alternative**: Claude Code through sbx's existing kit, pointed at Splash
with `ANTHROPIC_BASE_URL` (Splash speaks the Anthropic Messages API). Less
setup, but proprietary and tuned for Claude models. Kept as a fallback
harness behind the same seam.

## R6. Workspace exposure and host execution vectors

**Problem**: if the sandbox's workspace is the user's checkout, the agent
can write `.git/config` and `.git/hooks`. Then ordinary host git use
executes agent-authored commands:

- `core.fsmonitor` runs on `git status`
- `core.hooksPath` and `.git/hooks/*` run on commit, checkout and others
- `filter.*.clean` / `smudge`, `diff.external`, `core.pager`

Plus non-git vectors: `.envrc` (direnv), `.vscode/` tasks and settings,
Makefiles, package scripts.

**Decision**: the workspace is an **isolated clone** of the user's
checkout, in sesh's state directory, never the checkout itself:
- The host checkout's `.git` is never mounted (FR-010).
- Work comes back by `sesh pull`: sesh runs `git bundle create` *inside*
  the sandbox, then the host runs `git fetch <bundle>`. A bundle is pure
  data, and host git never reads the sandbox repo's config (FR-012).
- sesh never runs host git against the isolated clone.
- Documented: opening the isolated clone in an editor on the host
  re-creates the editor vector.

How sbx exposes a host directory (bind mount vs sync, and whether parts
can be excluded or read-only) decides the exact mechanics. **Spike**
(T010).

## R7. Search → SearXNG on the host

**Decision**: SearXNG as an OCI container on the host, published on
`127.0.0.1` only, JSON output enabled, reached by a pi extension through
an allowed `localhost:<port>` rule.

**Container runtime for host services**:
- Preferred: Apple `container` (1.0, one VM per container, no Docker
  Desktop needed; macOS 26 is already required by Splash).
- Fallback: Docker Desktop or OrbStack.
- Whether `container` reliably publishes to `127.0.0.1` and fits launchd
  supervision: **Spike** (T011).

**Accepted risk**: search queries leave the machine through SearXNG's
upstream engines, so repo content in queries can leak. Use the `offline`
profile for sensitive repositories.

**Not in v1**: arbitrary web fetch. The `online` profile adds a curated
allowlist of documentation hosts. A host-side "reader" tool can come
later as a custom tool (FR-020).

## R8. Implementation language → Python 3.12, `uv tool`

**Decision**: Python with a thin CLI (Typer), Pydantic for config, no
other runtime dependencies to start.

**Why**: the user is fluent in it, and the patterns carry over from
Sovereign (strict `extra="forbid"` config, single seams for external
CLIs, hermetic tests). sesh is mostly orchestration of other CLIs, so
Rust buys little here.

**Revisit if**: sesh ever owns a proxy or another long-running,
security-critical process.

## R9. SDD tooling → Spec Kit layout

**Decision**: follow GitHub Spec Kit's layout
(`.specify/memory/constitution.md`, `specs/NNN-feature/{spec,plan,
research,tasks}.md`, `contracts/`, `quickstart.md`). Spec Kit is at
v1.4.2 as of this writing.

The templates, scripts and agent commands haven't been generated yet;
the Spec Kit installer couldn't run where these docs were drafted. Run
`specify init --here` on the Mac to add them, and check that it keeps
the existing constitution.

## R10. Lessons carried from Sovereign

**Keep**:
- strict Pydantic config,
- one seam per external CLI, so tests mock it there,
- ADRs for contract changes,
- launchd for host services.

**Drop**:
- multi-engine routing,
- memory admission control (a single sum check is enough on 128 GB),
- telemetry wire protocols,
- dashboards,
- benchmark cells.

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
- Splash: [incoai/splash](https://github.com/incoai/splash),
  [model card](https://huggingface.co/incoai/Qwen3.8-27B-Splash),
  [Inco blog](https://inco.ai/blog/splash/)
- pi: [pi.dev](https://pi.dev/),
  [README (0.83)](https://unpkg.com/@earendil-works/pi-coding-agent@0.83.0/README.md),
  [providers doc](https://app.unpkg.com/@mariozechner/pi-coding-agent@0.73.1/files/docs/providers.md)
- Alternatives: [anthropics/sandbox-runtime](https://github.com/anthropics/sandbox-runtime),
  [microsandbox](https://github.com/superradcompany/microsandbox),
  [Strands Box](https://github.com/strands-agents/box),
  [Infisical Agent Vault](https://github.com/Infisical/agent-vault)
- [GitHub Spec Kit](https://github.github.com/spec-kit/)
