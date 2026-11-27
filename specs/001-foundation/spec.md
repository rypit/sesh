# Feature Specification: Foundation stack

**Feature Branch**: `001-foundation`

**Created**: 2026-11-26

**Status**: Draft

**Input**: User description: "Phase 1 — Foundation. Deliver the base stack every later phase builds on: one Compose file running Postgres with pgvector, Redis and Open WebUI; inference reached only through model aliases (agent-main) on any OpenAI-compatible server the operator points sesh at, with LiteLLM as the alias router so clients never see raw model names; people reach Open WebUI only over Tailscale; Open WebUI configured with built-in Memory off, raw models hidden from non-admin users, admin chat access off, task model = agent-main; a stub OpenAI-compatible model server for development and CI; the Rust workspace skeleton with a first `sesh doctor`; make check and CI. Acceptance: Open WebUI over Tailscale chats with agent-main; raw models hidden from a non-admin test user."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Chat with the household model from any device on the tailnet (Priority: P1)

A household member opens Open WebUI on their phone or laptop over
Tailscale, signs in, picks `agent-main` and has a conversation. Nothing in
the stack is reachable from the home network or the internet except
through Tailscale.

**Why this priority**: this is the first end-to-end path; every later
phase puts agents behind this same front end.

**Independent Test**: with the stack running against any
OpenAI-compatible server, sign in to Open WebUI through the tailnet
address and get a reply from `agent-main`; from another device on the
home network, the same port refuses the connection.

**Acceptance Scenarios**:

1. **Given** the stack is running and the operator has exposed Open WebUI
   on the tailnet, **When** a signed-in household member sends a message
   to `agent-main`, **Then** a reply streams back.
2. **Given** the stack is running, **When** any device on the home
   network connects to the server's LAN address on any published port,
   **Then** the connection is refused.
3. **Given** the inference server is down, **When** a member sends a
   message, **Then** Open WebUI shows an error rather than hanging, and
   recovers without a restart once the server is back.

---

### User Story 2 - Members see only sanctioned models (Priority: P1)

A non-admin household member sees only the aliases the operator has
allowed (`agent-main` for now), never the underlying model names, and the
operator cannot read other members' chats through the admin interface.

**Why this priority**: raw models bypass the agents, and admin chat
access breaks per-user privacy; both must be closed from the start.

**Independent Test**: sign in as a non-admin test user and list the
available models: only `agent-main` appears. As the admin, try to open
the test user's chats: it is not possible.

**Acceptance Scenarios**:

1. **Given** the inference server offers several raw models, **When** a
   non-admin member opens the model picker, **Then** only `agent-main` is
   listed.
2. **Given** a non-admin member has chats, **When** the admin looks for
   them in the admin interface, **Then** their content is not available.
3. **Given** default settings, **When** any member chats, **Then** Open
   WebUI's built-in memory stores nothing about them.
4. **Given** default settings, **When** Open WebUI generates a chat title
   or other background task, **Then** it uses `agent-main`.

---

### User Story 3 - Point sesh at any inference server (Priority: P1)

The operator chooses whatever OpenAI-compatible inference server they
run (on the server itself or elsewhere on the home network), tells sesh
its address and which of its models backs `agent-main`, and the stack
uses it. Swapping to a different server changes only that setting.

**Why this priority**: sesh must not depend on a particular engine
(constitution IX, ADR 0005).

**Independent Test**: run the stack against the stub server, then
against a second OpenAI-compatible server, changing only the inference
setting; Open WebUI keeps working with no other change.

**Acceptance Scenarios**:

1. **Given** an inference server address and model name in sesh's
   configuration, **When** the stack starts, **Then** `agent-main` is
   served by that model.
2. **Given** the stack is running, **When** the operator changes the
   inference setting and restarts the alias router, **Then** `agent-main`
   is served by the new server and no other component's configuration
   changed.
3. **Given** a client asks the alias router for a raw model name,
   **When** the request is made, **Then** it is refused.

---

### User Story 4 - Check the stack's health and safety in one command (Priority: P2)

The operator runs `sesh doctor` and gets a pass/fail list: each alias
answers, tool calling works with a short probe, every published port is
bound to loopback only, and each service is healthy. Any failure names
the fix.

**Why this priority**: the operator works from a phone much of the time;
one command must tell them whether the stack is sound.

**Independent Test**: run `sesh doctor` on a healthy stack: all checks
pass and it exits 0. Bind a port to all interfaces, or stop the inference
server: the matching check fails with a clear message and a non-zero
exit.

**Acceptance Scenarios**:

1. **Given** a healthy stack, **When** the operator runs `sesh doctor`,
   **Then** every check passes and the command exits 0.
2. **Given** a published port bound to all interfaces, **When** the
   operator runs `sesh doctor`, **Then** the loopback check fails and
   names the service and port.
3. **Given** a model that answers but cannot call tools, **When** the
   operator runs `sesh doctor`, **Then** the tool-calling check fails and
   says which alias.

---

### User Story 5 - Build and test without a real model (Priority: P2)

A developer (or CI) runs the whole stack with a stub inference server and
runs the checks and tests, with no GPU and no model download.

**Why this priority**: most of sesh is built away from the reference Mac,
including from a phone through cloud sessions.

**Independent Test**: on a machine with only a container runtime, run
the project's check and stack-test commands: they pass using the stub.

**Acceptance Scenarios**:

1. **Given** a machine with a container runtime and no inference server,
   **When** the developer runs the stack tests, **Then** they start the
   stack with the stub and pass.
2. **Given** a pull request, **When** CI runs, **Then** it runs the
   project checks, validates the Compose file, checks the architecture
   diagram and runs the stack tests.

### Edge Cases

- The inference server is unreachable at startup: the stack still
  starts; `agent-main` returns an error; `sesh doctor` fails the alias
  check.
- The configured model name doesn't exist on the inference server: the
  alias check fails and names the missing model.
- The inference server is slow (large model loading): requests wait up
  to a configured timeout, then fail with an error.
- A second Open WebUI account signs up uninvited: sign-ups are off by
  default; new accounts are created by the admin.
- Tailscale is not set up yet: the stack is reachable only from the
  server itself; `sesh doctor` reports Tailscale exposure as not
  configured, as a warning.
- A port the stack needs is already taken on the host: the stack fails to
  start that service and `sesh doctor` names the port.
- Data survives a restart: chats, accounts and database contents persist
  across a stack restart.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The stack MUST be defined in one Compose file and start with
  one command, running the database (with vector search), the cache, the
  chat front end and the alias router.
- **FR-002**: Every published port MUST bind to the loopback interface
  only; no service may be reachable from the LAN or internet except
  through the operator's Tailscale exposure.
- **FR-003**: Clients MUST reach models only through model aliases served
  by the alias router; the router MUST serve `agent-main` and refuse raw
  model names.
- **FR-004**: The inference server MUST be selected by configuration
  alone (base address, API key reference if any, and the model backing
  each alias); no component may depend on a particular engine.
- **FR-005**: The chat front end MUST have its built-in memory disabled,
  raw models hidden from non-admin users, admin access to other users'
  chats disabled, open sign-up disabled, and `agent-main` as its task
  model.
- **FR-006**: Data (database, front-end accounts and chats) MUST persist
  across restarts in named volumes outside this repository.
- **FR-007**: Local service credentials (database and cache passwords,
  the front end's secret key, the router's keys) MUST be generated on
  first setup, stored outside the repository, and never committed or
  logged.
- **FR-008**: A stub OpenAI-compatible server MUST be available for
  development and CI; it MUST support streamed chat, model listing and a
  deterministic tool call, so tests can exercise tool calling.
- **FR-009**: `sesh doctor` MUST check, and report pass/fail with a fix
  for each: every alias answers; tool calling works for every alias via a
  short probe; every published port is loopback-only; each service is
  healthy; whether Tailscale exposure is configured (warning only).
  It MUST exit non-zero if any check other than a warning fails.
- **FR-010**: `make check` MUST run formatting, lint, unit tests,
  dependency audit, the architecture-diagram check and Compose
  validation; a separate target MUST run the stack tests.
- **FR-011**: CI MUST run `make check` and the stack tests on every pull
  request.
- **FR-012**: The stack tests MUST include negative tests: a port bound
  beyond loopback is detected; a raw model name is refused; a non-admin
  user cannot list raw models; the admin cannot read another user's
  chats.

### Key Entities

- **Model alias**: a stable name clients use (`agent-main`), mapped to a
  model on the operator's inference server.
- **Inference setting**: the inference server's base address, optional
  key reference, and the model behind each alias.
- **Service**: one container in the stack, with its published ports (all
  loopback) and a health check.
- **Doctor check**: one named check with a result (pass, fail, warning)
  and a suggested fix.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A household member on the tailnet gets the first streamed
  reply from `agent-main` within 5 seconds of sending, when the inference
  server is warm.
- **SC-002**: From a device on the home network, 0 of the stack's ports
  accept a connection.
- **SC-003**: A non-admin test user sees exactly 1 model (`agent-main`).
- **SC-004**: Switching inference servers changes exactly 1 configuration
  setting group and no other component's configuration.
- **SC-005**: `sesh doctor` completes in under 30 seconds and flags each
  deliberately introduced fault (open port, missing model, no tool
  calling, stopped service) in 4 of 4 cases.
- **SC-006**: The full check and stack-test run passes on a machine with
  no GPU and no model download, in under 15 minutes in CI.

## Assumptions

- The operator has Tailscale on the server and exposes Open WebUI with
  `tailscale serve` (documented in the quickstart, not automated in this
  phase); `sesh doctor` only checks that it is configured.
- One inference server backs all aliases at first; `agent-main` is the
  only alias in this phase.
- The alias router is LiteLLM (ADR 0005), always present, so every
  inference server is handled the same way.
- Until the secret store arrives (Phase 4), local service credentials are
  generated into a file in sesh's data directory on the server
  (permissions 0600), outside the repository; Phase 4 moves them into the
  secret store.
- Open WebUI's first account is the admin; further accounts are created
  by the admin, so open sign-up stays off.
- Embedding models, agents (Hermes), the gateway and all resources are
  later phases; Postgres and Redis serve Open WebUI now and later
  components afterwards.
- Container images are pinned by version (and digest where available);
  upgrades are deliberate.
- CI runs on GitHub-hosted runners, which can pull public images.
