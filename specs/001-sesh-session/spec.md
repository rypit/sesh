# Feature Specification: One-command protected agent session

**Feature Branch**: `001-sesh-session`
**Created**: 2026-11-19
**Status**: Draft
**Input**: User description: "Clone a repo locally on my Mac, run one
command that brings up my local AI stack, loads the cwd into an
sbx-protected sandbox, and launches a harness (pi or similar) inside that
sandbox, connected to the local AI stack, which may later include custom
tools. Easy installation. Tight network, directory and secret isolation:
secrets only reach the tools that need them, only for the requests that
need them, and the agent never has direct access to them."

## Clarifications

### Session 2026-11-19

- Q: When `sesh` runs in a checkout with uncommitted or untracked changes, what does the sandbox workspace start from? → A: `HEAD` plus the uncommitted diff and untracked (non-ignored) files, copied into the isolated clone as one "sesh: carried changes" commit, automatically and without prompting.
- Q: With no flags, does a session get web search, package registries and configured APIs, or only the local model? → A: Default profile is `online`; `sesh --profile offline` (or `profile = "offline"` in user config) selects the local-model-only profile.
- Q: After a session ends, is the repository's sandbox kept or discarded? → A: Kept per repository between sessions; `sesh reset` discards it after listing unpulled work and confirming.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Start a protected session in a repo (Priority: P1)

A developer has cloned a repository. From its directory they run `sesh`.
The local AI stack starts if it isn't already running, a sandbox is
created (or reused) for the repository, and an interactive coding
harness opens inside the sandbox. The harness talks to the local model,
and the agent can read, write and run code in the repository's
workspace.

**Why this priority**: this is the product. Everything else hardens it or
extends it.

**Independent Test**: on a set-up Mac, run `sesh` in a sample repo and ask
the agent to add a function and run the test suite. The edit happens and
the tests run inside the sandbox, and the model's replies come from the
local engine.

**Acceptance Scenarios**:

1. **Given** the stack is stopped and no sandbox exists for the repo,
   **When** the user runs `sesh` in the repo, **Then** the stack starts,
   becomes healthy, a sandbox is created, and the harness opens connected
   to the local model.
2. **Given** the stack is running and the repo's sandbox exists, **When**
   the user runs `sesh`, **Then** the harness opens without restarting
   the stack or recreating the sandbox (warm start).
3. **Given** a session is open, **When** the agent runs a build or test
   command, **Then** it runs inside the sandbox, not on the host.
4. **Given** a session is open, **When** the agent tries to read a host
   path outside its workspace (e.g. `~/.ssh`), **Then** the read fails.
5. **Given** the user exits the harness, **When** they run `sesh` again,
   **Then** the agent's previous work in the workspace is still there.

---

### User Story 2 - Bring the agent's work back safely (Priority: P1)

After a session, the user reviews the agent's changes and brings them into
their own checkout, without anything the agent wrote being executed on
the host in the process.

**Why this priority**: Principle IV. Without this, the sandbox protects
the laptop only until the user's next `git commit`.

**Independent Test**: inside a session, have the agent plant a git hook,
a `core.fsmonitor` entry, an `.envrc` and a modified Makefile target, and
commit some code. Bring the work back with `sesh pull`, then run ordinary
git commands in the host checkout. None of the planted content executes,
and the code arrives as a reviewable branch.

**Acceptance Scenarios**:

1. **Given** the agent committed work in the sandbox, **When** the user
   runs `sesh pull`, **Then** the commits appear in the host checkout on
   branches named `sesh/<session>/<branch>` (one per branch in the
   sandbox), and the user's current branch and working tree are
   untouched.
2. **Given** the agent modified `.git/config` or `.git/hooks` in its
   workspace, **When** the user runs `sesh pull` and then any git command
   on the host, **Then** none of those modifications take effect on the
   host.
3. **Given** the agent left uncommitted changes, **When** the user runs
   `sesh pull`, **Then** sesh reports them and offers to have them
   committed inside the sandbox first, rather than silently dropping
   them.

---

### User Story 3 - Use the web and APIs without seeing secrets (Priority: P2)

The agent can search the web through the local search service, install
packages from public registries, push to GitHub and call other APIs the
user has allowed. Credentials are attached outside the sandbox, only on
requests to the hosts they belong to. Anything not allowed is blocked.

**Why this priority**: required for a real dev workflow ("call APIs,
search the web"), and it's where secret isolation is tested.

**Independent Test**: in a session, the agent searches the web, installs a
package, and runs `gh api user` with a configured GitHub credential. A
scan of the sandbox's environment, files and process list finds no real
credential value, and a request to a non-allowed host fails.

**Acceptance Scenarios**:

1. **Given** search is enabled, **When** the agent issues a web search,
   **Then** results come back through the local search service and the
   sandbox makes no direct connection to a search engine.
2. **Given** a GitHub credential is configured, **When** the agent calls
   the GitHub API or pushes over HTTPS, **Then** the request succeeds,
   and the sandbox only ever holds a placeholder.
3. **Given** a credential is bound to host A, **When** the agent sends
   its placeholder to host B, or in a header the runtime should not
   substitute, **Then** the real value is never sent to host B and never
   readable from any response.
4. **Given** a host is not on the allowlist, **When** the agent connects
   to it over any protocol (HTTP(S), raw TCP/UDP, DNS to an outside
   resolver), **Then** the connection is refused.
5. **Given** a method/path deny rule (e.g. `DELETE` on a GitHub repo),
   **When** the agent makes that request, **Then** it is refused even
   though the host is allowed.

---

### User Story 4 - Verify my protections (Priority: P2)

The user runs `sesh verify` and gets a pass/fail report of every security
invariant, checked from inside a real sandbox against the live stack.

**Why this priority**: Principle VI. It also gives the user evidence
after an upgrade of sbx, Splash or sesh.

**Independent Test**: run `sesh verify` on a correct setup: everything
passes. Deliberately loosen one policy (e.g. allow `example.com`), run it
again, and the matching check fails with a clear explanation.

**Acceptance Scenarios**:

1. **Given** a correct setup, **When** the user runs `sesh verify`,
   **Then** every invariant reports PASS and the command exits 0.
2. **Given** any invariant fails, **When** the user runs `sesh verify`,
   **Then** the failing invariant is named, with what was expected versus
   observed, and the command exits non-zero.
3. **Given** the user runs `sesh`, **When** the fast pre-launch checks
   (policy matches config, minimum versions) fail, **Then** the session
   does not start.

---

### User Story 5 - Easy installation (Priority: P3)

On a supported Mac, the user installs sesh and completes one-time setup
with at most three commands, plus waiting for model downloads.

**Why this priority**: important for adoption and for moving to a Mac
mini later, but the user can do it by hand initially.

**Independent Test**: on a fresh macOS user account that meets the
platform requirements, follow the quickstart. The first `sesh` session
works without editing any config file.

**Acceptance Scenarios**:

1. **Given** a supported Mac with Homebrew, **When** the user runs the
   install command and `sesh init`, **Then** all components are
   installed, the default model is downloaded, the sandbox runtime is
   authenticated, and `sesh doctor` passes.
2. **Given** an unsupported Mac (older chip or macOS, too little memory),
   **When** the user runs `sesh init`, **Then** it stops before changing
   anything and explains which requirement isn't met.
3. **Given** setup completed before, **When** the user runs `sesh init`
   again, **Then** it changes nothing and reports that everything is
   already set up.

---

### User Story 6 - Add a custom tool (Priority: P3)

The user adds a custom tool for the agent, e.g. one that needs a host
resource or a credential. It is declared in user config, follows the same
isolation rules as the built-in tools, and shows up in the harness.

**Why this priority**: explicitly a later need, but the design must not
block it. Built-in web search should be delivered through the same
mechanism, to prove it works.

**Independent Test**: declare a sample tool backed by a small host-side
service with a credential. The agent can call it, the service gets the
credential from the Keychain, and the sandbox never sees the credential.

**Acceptance Scenarios**:

1. **Given** a tool is declared in user config, **When** a session
   starts, **Then** the harness lists the tool and the sandbox policy
   allows exactly the tool's host-side endpoint, nothing more.
2. **Given** a tool is declared only inside the repository, **When** a
   session starts, **Then** it is ignored and sesh says so (Principle V).

---

### Edge Cases

The required behavior for each case is in FR-026 and FR-027.

- The cwd is not a git repository, or is a repo with no commits.
- The working tree has uncommitted or untracked changes when `sesh`
  starts: they are carried into the workspace as one commit (FR-011);
  ignored files are not.
- The repo uses submodules or Git LFS.
- Two `sesh` sessions are started for the same repo at the same time.
- The engine isn't installed, the model isn't downloaded, or there isn't
  enough free memory for the model.
- A configured port is already in use by something other than the stack.
- The sandbox runtime is not logged in, not running, or below the minimum
  version.
- The laptop sleeps, or the engine crashes, mid-session.
- The stack is running with a configuration older than the current one.
- The repository contains files that look like sesh or harness config
  (a malicious `.sesh*`, `AGENTS.md`, or pi config in the repo).
- No network connection at all (fully offline use with the local model).
- The harness exits abnormally; the agent's work must not be lost.
- An API needs a credential format the runtime can't inject (query
  parameter, request body, HTTP Basic, request signing).

## Requirements *(mandatory)*

### Functional Requirements

**Session lifecycle**

- **FR-001**: `sesh` run in a git repository MUST start the stack if
  needed, create or reuse that repository's sandbox, apply its policy,
  and open the harness, in one command with no required arguments.
- **FR-002**: The stack MUST stay running after a session ends, so other
  sessions can use it; `sesh stack down` stops it.
- **FR-003**: sesh MUST be idempotent: running a command again when the
  desired state already holds changes nothing.
- **FR-004**: Each repository MUST map to its own sandbox; sessions of
  different repositories MUST NOT share a sandbox or workspace.

**Isolation**

- **FR-005**: The sandbox MUST have no network egress except what the
  compiled policy allows: the local stack's endpoints, configured
  package registries, configured APIs, and configured tool endpoints.
- **FR-006**: The sandbox MUST NOT be able to reach the host's network
  services other than the endpoints sesh allows, nor the local network
  (router, NAS, other devices).
- **FR-007**: The sandbox MUST NOT have access to any host path except
  its workspace.
- **FR-008**: The agent MUST be able to run containers (`docker build`,
  test containers) inside its sandbox without any access to the host's
  container engine.
- **FR-009**: Policy MUST be compiled only from user-owned host config;
  repository content MUST NOT add allowances (Principle V).

**Workspace and returning work**

- **FR-010**: The sandbox's workspace MUST be set up so that nothing
  written inside it can execute on the host through git or any other
  tool the host runs implicitly (Principle IV). Specifically, the host
  checkout's `.git` directory MUST NOT be writable from the sandbox.
- **FR-011**: On session start, the sandbox workspace MUST start from the
  host checkout's `HEAD` plus its uncommitted changes and untracked,
  non-ignored files, recorded in the isolated clone as a single
  "sesh: carried changes" commit. This MUST happen without prompting and
  MUST NOT modify the host checkout. The carried commit's message MUST
  carry a `Sesh-Carried-Changes: yes` trailer, and `sesh pull` MUST list
  it separately from the agent's commits, so the user can drop it (the
  same changes are still uncommitted in their own tree).
- **FR-012**: `sesh pull` MUST transfer the agent's commits to the host
  checkout as data (e.g. a git bundle produced inside the sandbox) onto
  `sesh/<session>/<branch>` branches, without running host git against
  the sandbox's repository. sesh MUST record which commits it has pulled,
  so `sesh reset` can list unpulled work.

**Credentials**

- **FR-013**: Real credential values MUST NOT appear in the sandbox's
  environment, filesystem, process arguments, or in sesh's logs and
  state files.
- **FR-014**: Each credential MUST be bound to an explicit set of
  destination hosts, and attached only to requests to those hosts.
- **FR-015**: Credentials MUST be read from the macOS Keychain or a
  secrets manager at the time they're needed, not copied into sesh
  config or state.
- **FR-016**: For APIs whose auth the sandbox runtime can't inject,
  sesh MUST support a host-side shim (a local service that adds the
  credential and forwards only to its one upstream), exposed to the
  sandbox as a single allowed local endpoint. A shim is a stack service:
  launchd starts and supervises it, never the `sesh` CLI.

**Stack and harness**

- **FR-017**: The stack MUST include a local inference engine serving an
  OpenAI- and/or Anthropic-compatible API, and a local web search
  service. Both MUST listen only on the host's loopback interface.
- **FR-018**: The harness MUST be preconfigured to use the local engine,
  with no cloud model provider configured.
- **FR-019**: Web search MUST be available to the agent as a tool when
  the session's network profile is `online`. An `offline` profile MUST
  allow only the local engine and nothing else. The default profile
  MUST be `online`; `sesh --profile offline`, or `profile` in user
  config, MUST select `offline`, and
  `sesh policy show` MUST state which profile applies.
- **FR-020**: Tools MUST be declared in user config as: an optional
  host-side service, the endpoint the sandbox may reach, any credential
  bindings, and the harness integration. Built-in search MUST use the
  same mechanism.

**Verification and setup**

- **FR-021**: `sesh verify` MUST run every invariant check against a real
  sandbox and the live stack (from inside the sandbox, or on the host,
  as each check specifies) and report pass/fail per check, exiting
  non-zero on any failure.
- **FR-022**: Before opening a session, `sesh` MUST run fast checks (the
  applied policy matches the compiled policy; components meet minimum
  versions) and refuse to start if any fails.
- **FR-023**: `sesh init` MUST install and configure all components,
  stopping before making changes if the platform requirements aren't
  met.
- **FR-024**: `sesh policy show` MUST print the compiled policy for a
  repository's sandbox: allowed endpoints, method/path rules, credential
  bindings (names and hosts, never values).
- **FR-025**: The sandbox for a repository, including its workspace and
  caches, MUST be kept between sessions and reused on the next `sesh`
  run. `sesh reset` MUST discard it, listing unpulled work and asking for
  confirmation first.

**Edge-case behavior**

- **FR-026**: Only one harness MAY run in a repository's sandbox at a
  time. A second `sesh` in the same repository MUST refuse to start
  (exit 1), naming the running session. A lock left by a crashed `sesh`
  MUST be detected as stale and cleared.
- **FR-027**: sesh MUST handle these cases as stated:
  - Not a git repository, or a repository with no commits: refuse with
    a usage error (exit 2) that says what's missing.
  - Submodules or Git LFS: not supported in this feature. sesh MUST warn
    that submodules aren't initialized and LFS objects aren't fetched in
    the workspace, and continue.
  - Engine not installed, model missing, sandbox runtime logged out,
    stopped or too old, or too little free memory for the model: refuse
    before starting anything (exit 3) and name the fix.
  - A stack port in use by a process that isn't the expected service:
    refuse (exit 3), name the port and process, and never allow the
    sandbox to reach it.
  - The engine crashes, or the Mac sleeps, mid-session: launchd restarts
    the engine; the session stays open and its workspace is unaffected.
  - The running stack's configuration is older than the current config:
    refuse to open a session (exit 1), name the stale services, and tell
    the user to run `sesh stack up` to restart them.
  - Sesh- or harness-like config inside the repository: ignored for
    policy (FR-009); sesh says it was ignored.
  - No network connection: the `offline` profile works fully; in the
    `online` profile the session still opens, and only network calls
    fail.
  - The harness exits abnormally: the workspace is kept (FR-025), and
    `sesh` exits non-zero.

### Key Entities

- **Stack**: the host-side services (inference engine, search service,
  tool shims) shared by all sessions. Has a state (stopped, starting,
  healthy, degraded).
- **Service**: one host-side process in the stack (engine, search, or a
  `sesh-shim`), supervised by launchd, with a loopback-only endpoint and
  a health check.
- **Sandbox**: an isolated environment for one repository, with a
  workspace, an applied policy, and a lifecycle. It persists across
  sessions until `sesh reset`.
- **Session**: a line of work in a sandbox, with a short id unique per
  repository that names the branches its work returns on. A session
  begins on the first `sesh` run in a new sandbox and continues across
  later runs until `sesh --new` or `sesh reset` starts another.
- **Policy**: the compiled set of allowed endpoints, method/path rules,
  and credential bindings for a sandbox.
- **Credential binding**: a credential name, its source (Keychain item or
  secrets-manager reference), the hosts it's bound to, and how it's
  attached (runtime injection or shim).
- **Profile**: a named policy preset (`online`, the default, or
  `offline`).
- **Tool**: an agent capability made of a harness integration plus,
  optionally, a host service, an allowed endpoint, and credential
  bindings.
- **Invariant**: one executable security check, with the principle it
  verifies and an expected outcome.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the stack running and the sandbox existing, `sesh` in
  a repo reaches the harness prompt in under 10 seconds. A cold start
  (stack stopped, new sandbox), excluding downloads, takes under 90
  seconds.
- **SC-002**: On the reference Mac, `sesh verify` passes 100% of
  invariants, and each invariant has been seen to fail when its
  protection is removed.
- **SC-003**: Across the verify suite, zero real credential values are
  observable from inside the sandbox (environment, filesystem, process
  list, echo-server responses).
- **SC-004**: From a supported Mac with Homebrew, the first session works
  after at most 3 commands, not counting download time, with no config
  file edits.
- **SC-005**: In the planted-content test (US2), zero agent-authored
  files execute on the host during `sesh pull` and normal git use
  afterwards.
- **SC-006**: Asked to "search the web for X and summarize it", the agent
  succeeds with the sandbox making zero direct connections to the
  internet.
- **SC-007**: sesh runs no background process of its own; with no session
  open, its only resident processes are the stack's launchd-managed
  services (engine, search, `sesh-shim` instances).

## Assumptions

- Reference machine: MacBook Pro, M5 Max, 128 GB, macOS 26.4 or later.
  The design should move to a Mac mini or Studio later without changes.
- Single user, single machine. Remote access, multiple users and the
  always-on home agent are out of scope for this feature.
- Sandbox runtime: Docker Sandboxes (`sbx`), chosen for its microVM per
  sandbox, deny-by-default egress proxy, host-side credential injection
  and per-sandbox Docker engine (see research.md). Its experimental
  custom secrets and known gaps are accepted and covered by shims and
  invariants.
- Inference engine: Splash by default, oMLX as fallback for models
  Splash doesn't support.
- Harness: pi, with no built-in MCP support (tools come through pi
  extensions or skills).
- Search: SearXNG on the host.
- Data at risk in an online session is mainly the repository's code
  (e.g. leaked through search queries). This is accepted for development
  use; the `offline` profile exists for sensitive repositories.

## Out of Scope

- Multiple engines at once, automatic model routing, benchmarks,
  dashboards (Sovereign's territory).
- Voice models (Whisper, Kokoro); they join the stack in a later feature
  as tools under FR-020.
- The always-on home agent, messaging channels, schedulers, remote
  access over Tailscale.
- Approval prompts before risky requests (planned for a later feature;
  needs an upstream proxy).
- Linux or Windows hosts.
