> **Deferred (dev mode).** Since constitution 2.0.0 and ADR 0006, sesh is
> a household agent platform; this feature, a sandbox for terminal coding
> agents, is parked as a possible later "dev mode". It is kept for
> reference and is not being implemented. Its sbx research (research.md)
> still informs the browser phase.

# Feature Specification: Sandboxed agent sessions for any terminal harness

**Feature Branch**: `001-sesh-session`

**Created**: 2026-11-19 (rewritten 2026-11-19)

**Status**: Deferred

**Input**: User description: "An easy way to sandbox agents. A dev tool
that makes running any terminal coding agent as easy and ergonomic as
running `claude` directly, but inside a microVM sandbox so a
prompt-injected agent can't harm the Mac. sesh is harness-agnostic
(harness knowledge lives in small data-only recipes), imports the user's
own harness config read-only with credentials stripped, attaches
credentials outside the sandbox, treats everything the sandbox may reach
as a user-declared endpoint, returns work automatically as data, and
verifies its guarantees. Local inference is one option, not the point."

## Clarifications

### Session 2026-11-19

- Q: When `sesh` runs in a checkout with uncommitted or untracked changes, what does the sandbox workspace start from? → A: `HEAD` plus the uncommitted diff and untracked (non-ignored) files, copied into the isolated clone as one "sesh: carried changes" commit, automatically and without prompting.
- Q: After a session ends, is the sandbox kept or discarded? → A: Kept between sessions; `sesh reset` discards it after listing unpulled work and confirming.
- Q: Does sesh bring up a local AI stack (inference engine, search)? → A: No. Inference, search and every other service are endpoints the user runs or subscribes to; sesh only allows them, attaches their credentials and tells the sandbox where they are. (Supersedes the earlier "default profile `online`" answer: profiles are replaced by per-repository endpoint selection, FR-018.)
- Q: Does sesh assume a particular harness? → A: No. Any terminal harness works through a recipe, a small data-only description; sesh ships some and users can write their own.
- Q: How should work come back? → A: Automatically when the harness exits, as reviewable branches, with risky paths highlighted; no separate step is required.
- Q: Are web-UI agents or always-on agents in scope? → A: No. sesh is a dev tool for terminal harnesses. Recipes leave room for other workspace and attach modes later, but this feature builds only a repository workspace attached to the terminal.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run my agent in a sandbox as easily as running it directly (Priority: P1)

A developer who normally types `claude` (or `codex`, `opencode`, `pi`…)
in a repository types `sesh claude` instead, or just `sesh` once a
default is set. Their usual harness opens in the terminal with their own
settings, skills and tool configuration, already logged in to their
model provider, working on their repository. It looks and behaves like
the harness they know, but it runs inside a sandbox that can't read the
rest of their Mac and can reach only what they've allowed.

**Why this priority**: this is the product. If it isn't as easy as
running the harness directly, people won't use it.

**Independent Test**: on a set-up Mac with a harness the user already
uses, run `sesh <recipe>` in a sample repository. The harness opens with
the user's own settings, answers through the user's model provider,
edits a file and runs the tests, all inside the sandbox, and `~/.ssh`
is not readable from it.

**Acceptance Scenarios**:

1. **Given** a user who already uses a harness with a built-in recipe,
   **When** they run `sesh <recipe>` in a repository for the first time,
   **Then** the harness opens in their terminal, with their own harness
   settings, and can reach their configured model endpoint without any
   change to their harness config.
2. **Given** the repository's sandbox already exists, **When** the user
   runs `sesh <recipe>` again, **Then** the harness opens without the
   sandbox being recreated (warm start), with the previous session's
   work and installed dependencies still there.
3. **Given** a default recipe is set, **When** the user runs `sesh` with
   no arguments, **Then** that recipe runs; and **When** the user passes
   arguments after the recipe name, **Then** they reach the harness
   unchanged.
4. **Given** a session is open, **When** the agent runs a build, test or
   container command, **Then** it runs inside the sandbox, not on the
   host.
5. **Given** a session is open, **When** the agent tries to read a host
   path outside its workspace (e.g. `~/.ssh`, the user's real harness
   config directory), **Then** the read fails.
6. **Given** a session is open, **When** the agent changes the harness
   configuration it sees (e.g. adds a hook or a tool server), **Then**
   the user's real harness config on the host is unchanged, and the
   change is gone at the next session.

---

### User Story 2 - Get the agent's work back without ceremony, and safely (Priority: P1)

When the user exits the harness, the agent's work is already back in
their own checkout as branches they can review and merge with ordinary
git. Nothing the agent wrote runs on the host in the process, and files
that could run code implicitly are pointed out.

**Why this priority**: Principle IV, and ergonomics. Without automatic
return the sandbox feels like friction; without safe return it protects
the laptop only until the next `git commit`.

**Independent Test**: in a session, have the agent plant a git hook, a
`core.fsmonitor` entry, an `.envrc`, a modified Makefile target and a
CI file, and commit some code. Exit the harness. The work appears as a
branch in the host checkout, the risky files are listed, and running
ordinary git commands on the host executes none of the planted content.

**Acceptance Scenarios**:

1. **Given** the agent committed work, **When** the harness exits,
   **Then** the commits appear in the host checkout on
   `sesh/<session>/<branch>` branches, the user's current branch and
   working tree are untouched, and sesh prints a short summary.
2. **Given** the agent's commits touch files that can run code
   implicitly (e.g. `.envrc`, editor task or settings files, Makefiles,
   CI configuration, package-manager scripts, git attributes), **When**
   the work returns, **Then** the summary lists those files.
3. **Given** the agent modified git configuration or hooks in its
   workspace, **When** the work returns and the user runs any git
   command on the host, **Then** none of those modifications take
   effect on the host.
4. **Given** the agent left uncommitted changes, **When** the harness
   exits, **Then** sesh reports them and offers to commit them inside
   the sandbox and return them, rather than silently dropping them.
5. **Given** the user started with uncommitted changes, **When** the
   work returns, **Then** the carried-changes commit is listed
   separately from the agent's commits.

---

### User Story 3 - Give the agent exactly the access it needs, without exposing secrets (Priority: P1)

The user declares, in their own config, what the sandbox may reach: their
model provider, a local inference server, a search service, package
registries, APIs. Credentials for those are attached outside the sandbox,
only on requests to the hosts they belong to. Everything else is blocked.
When something the agent needed was blocked, sesh says so and one
command allows it.

**Why this priority**: deny-by-default is the security core, and without
an easy way to grant access it's also the main source of friction.

**Independent Test**: in a session, the agent calls the model provider,
installs a package and calls a configured API. A scan of the sandbox's
environment, files and process list finds no real credential value. A
request to an unlisted host fails, and after exit sesh lists that host;
`sesh allow <host>` makes the next session succeed.

**Acceptance Scenarios**:

1. **Given** a model provider endpoint with a credential is configured,
   **When** the harness calls it, **Then** the request succeeds and the
   sandbox only ever holds a placeholder.
2. **Given** a credential is bound to host A, **When** the agent sends
   its placeholder to host B, **Then** the real value is never sent to
   host B and never readable from any response.
3. **Given** a host is not allowed, **When** the agent connects to it
   over any protocol, **Then** the connection is refused.
4. **Given** connections were refused during a session, **When** the
   session ends, **Then** sesh lists the refused hosts; and **When** the
   user runs `sesh allow <host>`, **Then** that host is added to the
   user's config and allowed from the next session on.
5. **Given** a method/path deny rule (e.g. `DELETE` on a repository
   API), **When** the agent makes that request, **Then** it is refused
   even though the host is allowed.
6. **Given** an endpoint is a local service on the Mac (e.g. a local
   inference server), **When** it is declared, **Then** the sandbox can
   reach exactly that service and no other host port or LAN address.

---

### User Story 4 - Verify my protections (Priority: P2)

The user runs `sesh verify` and gets a pass/fail report of every
security invariant, checked against a real sandbox, whatever harness
they use.

**Why this priority**: Principle VI, and evidence after upgrading the
sandbox runtime, a harness, or sesh.

**Independent Test**: run `sesh verify` on a correct setup: everything
passes. Loosen one protection (e.g. allow `example.com`), run it again,
and the matching check fails with a clear explanation.

**Acceptance Scenarios**:

1. **Given** a correct setup, **When** the user runs `sesh verify`,
   **Then** every invariant reports PASS and the command exits 0.
2. **Given** any invariant fails, **When** the user runs `sesh verify`,
   **Then** the failing invariant is named, with expected versus
   observed, and the command exits non-zero.
3. **Given** the user runs `sesh`, **When** the fast pre-launch checks
   (applied policy matches config, minimum versions) fail, **Then** the
   session does not start.

---

### User Story 5 - Bring a harness sesh doesn't know yet (Priority: P2)

The user wants to use a harness with no built-in recipe. They write a
short recipe in their own config directory, describing how to install
and launch it, where its config lives, which of its files hold
credentials, and how it finds its model endpoint. It then works like a
built-in one.

**Why this priority**: "any harness" depends on it, but the built-in
recipes cover the common case first.

**Independent Test**: write a recipe for a harness without a built-in
one, using only the documented recipe format, and run a session with
it that meets US1's acceptance scenarios.

**Acceptance Scenarios**:

1. **Given** a valid recipe in the user's config directory, **When** the
   user runs `sesh <name>`, **Then** the harness runs as in US1.
2. **Given** a recipe declares credential files, **When** its config is
   imported, **Then** those files are not in the sandbox.
3. **Given** a recipe or harness-like config exists only inside the
   repository, **When** a session starts, **Then** it is ignored for
   sesh's purposes and sesh says so (Principle V).
4. **Given** an invalid recipe, **When** the user runs it, **Then** sesh
   refuses and names the problem.

---

### User Story 6 - Easy installation (Priority: P3)

On a supported Mac, the user installs sesh and completes one-time setup
with at most three commands, and `sesh init` sets a default recipe and
their model endpoint by asking, not by config editing.

**Why this priority**: important for adoption; the user can set up by
hand initially.

**Independent Test**: on a fresh macOS account that meets the platform
requirements, follow the quickstart; the first session works without
editing any file.

**Acceptance Scenarios**:

1. **Given** a supported Mac, **When** the user runs the install command
   and `sesh init`, **Then** the sandbox runtime is installed and
   authenticated, a default recipe and model endpoint are chosen
   interactively, credentials are stored in the Keychain, and
   `sesh doctor` passes.
2. **Given** an unsupported Mac, **When** the user runs `sesh init`,
   **Then** it stops before changing anything and explains which
   requirement isn't met.
3. **Given** setup completed before, **When** the user runs `sesh init`
   again, **Then** it changes nothing and says so.

---

### Edge Cases

The required behavior for each case is in FR-026 and FR-027.

- The cwd is not a git repository, or is a repo with no commits.
- The working tree has uncommitted or untracked changes when the
  session starts (FR-011).
- The repo uses submodules or Git LFS.
- Two sessions are started for the same sandbox at the same time.
- An endpoint isn't reachable (e.g. the local inference server isn't
  running), with or without a configured start command.
- A local endpoint's port is held by a different process than expected,
  or listens beyond loopback.
- The sandbox runtime is not logged in, not running, or below the
  minimum version.
- The harness needs hosts its recipe didn't declare (telemetry, update
  checks, tool APIs) and misbehaves when they're blocked.
- The user's harness config references local tool servers whose
  programs don't exist inside the sandbox.
- The user's harness config contains a credential the recipe didn't
  list (a token pasted into a settings file).
- The harness's login can't be attached outside the sandbox (e.g. a
  subscription login the runtime can't inject).
- The repository contains files that look like sesh, recipe or harness
  config.
- The harness exits abnormally; the agent's work must not be lost.
- An API needs a credential format the runtime can't inject (query
  parameter, request body, HTTP Basic, request signing).

## Requirements *(mandatory)*

### Functional Requirements

**Sessions and recipes**

- **FR-001**: `sesh <recipe> [args…]` run in a git repository MUST
  create or reuse the sandbox for that recipe and repository, apply its
  policy, and run the recipe's harness attached to the terminal, passing
  `args` through unchanged. `sesh` with no arguments MUST run the
  user's default recipe.
- **FR-002**: Harness-specific knowledge MUST live only in recipes:
  data describing how to install and launch the harness, which host
  config paths to import, which of those hold credentials, the
  endpoints and credentials the harness needs, and how the harness is
  told where its endpoints are. sesh's behavior MUST NOT otherwise
  depend on which harness runs.
- **FR-003**: sesh MUST ship built-in recipes for at least Claude Code,
  Codex CLI, opencode and pi, and MUST accept user recipes from the
  user's config directory in the same, documented format. A user recipe
  with the same name overrides a built-in one.
- **FR-004**: A recipe MAY declare a `workspace` kind and an `attach`
  mode. This feature MUST support `workspace = repo` with
  `attach = terminal` only, and MUST refuse any other value with a
  clear message.
- **FR-005**: Each (recipe, repository) pair MUST map to its own sandbox;
  sandboxes MUST NOT share a workspace.
- **FR-006**: sesh MUST be idempotent: running a command again when the
  desired state already holds changes nothing.

**Harness configuration import**

- **FR-007**: At each session start, sesh MUST copy the host config
  paths the recipe names into the sandbox as a snapshot that the
  sandbox cannot use to change the host copy. Changes made inside the
  sandbox MUST NOT be written back to the host, and MUST be replaced by
  a fresh snapshot at the next session.
- **FR-008**: The snapshot MUST exclude every file the recipe lists as
  holding credentials. sesh MUST also scan the snapshot for
  high-confidence credential patterns, exclude any match, and tell the
  user which file was excluded and why.

**Isolation**

- **FR-009**: The sandbox MUST have no network egress except to the
  endpoints the compiled policy allows.
- **FR-010**: The sandbox MUST NOT reach host network services other
  than declared local endpoints, nor the local network.
- **FR-011**: The sandbox MUST NOT have access to any host path except
  its workspace and the config snapshot. On session start the
  workspace MUST start from the host checkout's `HEAD` plus its
  uncommitted changes and untracked, non-ignored files, recorded as a
  single "sesh: carried changes" commit with a
  `Sesh-Carried-Changes: yes` trailer, without prompting and without
  modifying the host checkout.
- **FR-012**: The agent MUST be able to run containers inside its
  sandbox without access to the host's container engine.
- **FR-013**: Policy MUST be compiled only from user-owned host
  configuration and the recipes installed with sesh or in the user's
  config directory; repository content MUST NOT add allowances
  (Principle V).

**Returning work**

- **FR-014**: The host checkout's `.git` MUST NOT be reachable from the
  sandbox, and nothing written in the sandbox may execute on the host
  through git or any tool the host runs implicitly (Principle IV).
- **FR-015**: When the harness exits, sesh MUST return the agent's
  commits to the host checkout as data onto `sesh/<session>/<branch>`
  branches, without running host git against the sandbox's repository,
  print a summary that lists the carried-changes commit separately and
  highlights files that can run code implicitly, and record what was
  returned. `sesh pull` MUST do the same on demand.

**Endpoints and credentials**

- **FR-016**: Every service the sandbox may reach (model providers,
  local inference servers, search, tool APIs, package registries) MUST
  be an endpoint declared in user config or a recipe: a host (or a
  local port), optional method/path rules, and optional credential
  bindings. sesh MUST NOT install or start inference or search services
  itself, except by running an endpoint's optional, user-configured
  start command when the endpoint is unreachable.
- **FR-017**: sesh MUST tell the harness where its endpoints are as the
  recipe specifies (environment variables or a generated config file
  inside the sandbox).
- **FR-018**: User config MUST be able to restrict a repository to a
  subset of endpoints. Restrictions MAY only narrow, never widen, what
  recipes and global config allow.
- **FR-019**: Real credential values MUST NOT appear in the sandbox's
  environment, filesystem, process arguments, or in sesh's logs and
  state files. Each credential MUST be bound to an explicit set of
  destination hosts and attached only to requests to those hosts. It
  MUST be read from the OS credential store (the macOS Keychain) or a
  secrets manager when needed.
- **FR-020**: For credentials the sandbox runtime can't inject, sesh
  MUST support a host-side shim (a local service that adds the
  credential and forwards only to its one upstream), exposed to the
  sandbox as a single local endpoint and supervised by the OS service
  manager (launchd on macOS), never by
  the `sesh` CLI. A harness login that can be attached neither way is
  unsupported, and sesh MUST say so rather than place the credential in
  the sandbox.
- **FR-021**: After each session, sesh MUST list the hosts that were
  refused. `sesh allow <host>` MUST add an endpoint for that host to the
  user's config, scoped to the current repository unless `--global` is
  given, and MUST print what it added.

**Verification and setup**

- **FR-022**: `sesh verify` MUST run every invariant check against a
  real sandbox (from inside it, or on the host, as each check
  specifies), independently of any harness, and report pass/fail per
  check, exiting non-zero on any failure.
- **FR-023**: Before opening a session, sesh MUST run fast checks (the
  applied policy matches the compiled policy; components meet minimum
  versions; endpoints marked required answer) and refuse to start if any
  fails.
- **FR-024**: `sesh init` MUST install and authenticate the sandbox
  runtime, interactively choose a default recipe and model endpoint,
  store credentials in the Keychain, and stop before making changes if
  the platform requirements aren't met.
- **FR-025**: `sesh policy show` MUST print the compiled policy for the
  current repository's sandbox: endpoints and where each came from
  (recipe, global config, repository scope), method/path rules, and
  credential bindings (names and hosts, never values). The sandbox for a
  (recipe, repository) pair MUST be kept between sessions; `sesh reset`
  MUST discard it after listing unreturned work and asking for
  confirmation.

**Edge-case behavior**

- **FR-026**: Only one harness MAY run in a sandbox at a time. A second
  session for the same sandbox MUST refuse to start (exit 1), naming the
  running session. A lock left by a crashed run MUST be detected as
  stale and cleared.
- **FR-027**: sesh MUST handle these cases as stated:
  - Not a git repository, or no commits: refuse with a usage error
    (exit 2) that says what's missing.
  - Submodules or Git LFS: not supported in this feature; warn that
    submodules aren't initialized and LFS objects aren't fetched, and
    continue.
  - Sandbox runtime logged out, stopped or below the minimum version:
    refuse before starting anything (exit 3) and name the fix.
  - A required endpoint unreachable: run its start command if one is
    configured and wait for it; otherwise, or if it still fails, refuse
    (exit 3) and name the endpoint.
  - A local endpoint's port bound beyond loopback, or held by a process
    other than the one configured: refuse (exit 3) and name the port and
    process.
  - Harness or repository config that looks like sesh or recipe config
    inside the repository: ignored for policy (FR-013); sesh says so.
  - A tool server in the imported harness config whose program isn't in
    the sandbox: left to the harness to report; sesh does not install
    it.
  - The harness exits abnormally: the workspace is kept, work is
    returned as in FR-015, and sesh exits non-zero.

### Key Entities

- **Recipe**: data describing one harness: install source, launch
  command, config paths to import, credential files to exclude,
  endpoints and credentials it needs, how it learns endpoint locations,
  and its workspace kind and attach mode. Built-in or user-provided;
  never from a repository.
- **Endpoint**: something the sandbox may reach: a host or local port,
  optional method/path rules, optional credential bindings, optional
  start command, and whether it is required. Declared globally, per
  repository, or by a recipe.
- **Credential binding**: a credential name, its source (Keychain item or
  secrets-manager reference), the hosts it's bound to, and how it's
  attached (runtime injection or shim).
- **Shim**: a launchd-supervised host service that attaches one
  credential and forwards to one upstream.
- **Sandbox**: an isolated environment for one (recipe, repository)
  pair, with a workspace, a config snapshot, an applied policy and a
  lifecycle. It persists until `sesh reset`.
- **Config snapshot**: the read-only copy of a recipe's host config
  paths for one session, with credentials removed.
- **Session**: a line of work in a sandbox, with a short id unique per
  repository that names the branches its work returns on. It continues
  across runs until `sesh --new` or `sesh reset` starts another.
- **Policy**: the compiled set of allowed endpoints, method/path rules,
  and credential bindings for a sandbox.
- **Invariant**: one executable security check, with the principle it
  verifies and an expected outcome.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the sandbox existing, `sesh <recipe>` reaches the
  harness prompt in under 10 seconds. A first session in a new
  repository, excluding downloads, reaches it in under 90 seconds.
- **SC-002**: For each built-in recipe, a user who already uses that
  harness can start their first sandboxed session with zero edits to
  their harness config.
- **SC-003**: On the reference Mac, `sesh verify` passes 100% of
  invariants, and each invariant has been seen to fail when its
  protection is removed.
- **SC-004**: Across the verify suite and each built-in recipe, zero real
  credential values are observable from inside the sandbox
  (environment, filesystem including the config snapshot, process list,
  echo-server responses).
- **SC-005**: In the planted-content test (US2), and in a test that
  plants a hook in the imported harness config, zero agent-authored
  files execute on the host during return and normal use afterwards.
- **SC-006**: Returned work is available in the host checkout within
  5 seconds of the harness exiting, for a repository of 10,000 files.
- **SC-007**: Every host refused during a session appears in the
  post-session list, and after `sesh allow` the next session reaches it.
- **SC-008**: From a supported Mac, the first session works after at most
  3 commands, not counting downloads, with no file edits.
- **SC-009**: sesh runs no background process of its own; with no session
  open, its only resident processes are launchd-managed shims.

## Assumptions

- Reference machine: MacBook Pro, Apple Silicon, recent macOS. Platform
  requirements are those of the sandbox runtime (macOS 14+ on Apple
  Silicon), not of any inference engine. The sandbox runtime also runs on
  Linux and Windows, so macOS-only parts of sesh are confined to its
  credential-store and service-manager seams (ADR 0005).
- Single user, single machine.
- Sandbox runtime: Docker Sandboxes (`sbx`), chosen for its microVM per
  sandbox, deny-by-default egress proxy, host-side credential injection
  and per-sandbox container engine (ADR 0001). Whether it can attach
  subscription logins (rather than API keys) for harnesses that use
  them is unknown and will be established by a spike; API-key logins
  are assumed to work.
- Endpoints are run by the user or a provider: a cloud model API, a
  local inference server (Splash, oMLX, Ollama, LM Studio, or a stack
  manager like Sovereign), a search service, etc. sesh is indifferent to
  which.
- Built-in recipes target the harnesses' current releases; recipes are
  updated when a harness moves its config or credential files.
- Data at risk in a session is mainly the repository's code, sent to
  whatever endpoints the user allowed. Choosing those endpoints is the
  user's decision; per-repository restriction exists for sensitive
  repositories.

## Out of Scope

- Web-UI agents (e.g. Open WebUI), and any `attach` mode other than the
  terminal.
- Always-on or scheduled agents (e.g. Hermes-like personal agents),
  messaging channels, and any `workspace` kind other than a repository.
- Starting, routing or benchmarking inference engines; dashboards.
- Approval prompts before risky requests (planned later; needs an
  upstream proxy).
- Remote access, multiple users.
- Linux hosts (planned for a later feature; this feature keeps OS-specific
  code behind seams) and Windows hosts (not planned).
