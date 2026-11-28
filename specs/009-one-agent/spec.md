# Feature Specification: One agent end to end

**Feature Branch**: `claude/one-agent`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Proceed with 009 (one agent end to end): one Hermes profile in a container, with built-in tools off and the gateway as its only MCP server; Open WebUI connected to it. Accept: chatting in Open WebUI, the agent answers a weather and a search question, and uses mail, documents and feeds via the gateway; attempts to use terminal or file tools fail. Memory comes in phase 11. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **A household member** chats with the agent in Open WebUI.
- **The operator** provisions the agent with sesh's CLI.
- **An attacker** controls content the agent reads (a web page, an
  email, a feed item, a document) or the model's output, and tries to
  make the agent act outside the gateway.

Until now, sesh's agents were identities only: test clients and the
stub model exercised the gateway. This feature runs the first real
agent, the Household agent, as an adopted agent runtime that household
members chat with in Open WebUI, and whose only way to do anything is
the gateway.

### User Story 1 - Chat with the Household agent in Open WebUI (Priority: P1) 🎯 MVP

A household member opens Open WebUI, picks the Household agent and asks
"what's the weather at home tomorrow?" and "search the web for X". The
agent answers using the gateway's weather and search, in the same chat,
with the answer streaming in.

**Why this priority**: It's the first time sesh is usable as an
assistant rather than a set of tested parts.

**Independent Test**: With the stub model playing a scripted
conversation, a chat request through Open WebUI to the Household agent
produces an answer that contains facts only the gateway's fake weather
and search could have supplied, and the gateway's audit log shows the
calls.

**Acceptance Scenarios**:

1. **Given** a running stack with the Household agent, **When** a
   household member lists models in Open WebUI, **Then** the Household
   agent is there.
2. **Given** a chat with the Household agent, **When** the member asks a
   weather question, **Then** the answer contains the gateway's
   forecast, and the audit log shows one `geo.forecast` call by that
   agent.
3. **Given** a chat, **When** the member asks a search question,
   **Then** the answer cites a result from the gateway's search.
4. **Given** a chat, **When** the answer is long, **Then** it streams
   into the chat as it's produced.

---

### User Story 2 - The agent uses the household's accounts through the gateway (Priority: P1)

The agent answers questions about the household's mail, documents and
feeds ("anything from the electricity company?", "what's new on the
harbour feed?"), and asks for approval before sending mail, exactly as
a gateway client does today.

**Why this priority**: These are the resources features 005 and 006
built; the agent is the first real user of them.

**Independent Test**: Against fake Google, the stub Paperless or the
real one, and the fake feed publisher, scripted chats read mail, a
document and a feed item through the gateway; a send produces an
approval request on the Household topic and nothing is sent until it's
approved.

**Acceptance Scenarios**:

1. **Given** a connected Household Google account, **When** the member
   asks about recent mail, **Then** the answer is based on the
   gateway's `mail.search` result.
2. **Given** a feed the Household follows, **When** the member asks
   what's new, **Then** the answer cites the feed item.
3. **Given** the member asks the agent to send an email, **When** the
   agent calls send, **Then** an approval request reaches the Household
   topic and no mail is sent until the person approves.

---

### User Story 3 - The agent has no way out except the gateway (Priority: P1)

The agent runtime's own tools (terminal, files, web, browser, code
execution, image generation, memory and the rest) are off, and the
agent's container can reach nothing but the gateway, the model router
and the chat UI that talks to it. A prompt-injected agent that tries to
run a command, read a file, fetch a URL or reach another service gets
nowhere.

**Why this priority**: Constitution II (one way out) and I (agents are
untrusted). This is the guarantee that makes running an agent safe.

**Independent Test**: The tool list the agent runtime sends to the
model contains only the gateway's tools. A scripted model reply that
asks for a terminal, file or web tool is refused by the runtime. From
inside the agent's container, every address except the three allowed
destinations is unreachable, including the internet.

**Acceptance Scenarios**:

1. **Given** a chat, **When** the agent runtime sends a request to the
   model, **Then** the tools offered are exactly the gateway's.
2. **Given** a model reply that calls a terminal, file, web or code
   tool, **When** the runtime handles it, **Then** no such tool runs,
   and nothing outside the gateway is reached.
3. **Given** the agent's container, **When** it tries to connect to the
   internet, the database, the secret store, the approval channel,
   another agent or an executor, **Then** every attempt fails.
4. **Given** the agent's container, **When** its configuration and
   credentials are inspected, **Then** it holds only its own gateway
   token and model-router key, never a resource credential.

---

### User Story 4 - The operator adds the agent with one command (Priority: P2)

The operator runs one command to give an existing agent record a
running agent runtime and makes it appear in Open WebUI; `sesh up`
starts it, `sesh doctor` reports it, and removing the agent stops it
and removes it from Open WebUI.

**Why this priority**: Provisioning must stay one step (the
architecture's operator model), and it is the base 012 builds on.

**Independent Test**: `sesh agent add household-agent --runtime hermes
…` then `sesh up`: the agent's container is running and healthy, it is
listed in Open WebUI, and doctor's checks pass; `sesh agent remove`
then `sesh up`: the container is gone and the model no longer listed.

**Acceptance Scenarios**:

1. **Given** the operator adds an agent with the agent runtime, **When**
   `sesh up` runs, **Then** the agent's container starts healthy and
   the agent is listed in Open WebUI.
2. **Given** a running agent, **When** `sesh doctor` runs, **Then** it
   reports the agent's runtime healthy and its tools limited to the
   gateway.
3. **Given** the operator removes the agent, **When** `sesh up` runs,
   **Then** the container and its Open WebUI entry are gone and its
   gateway token no longer works.

### Edge Cases

- **The model calls a tool the runtime doesn't offer**: it's refused by
  the runtime and, since the container has no other route, could reach
  nothing anyway.
- **The gateway is down**: the agent says it can't reach its tools; it
  never falls back to anything else.
- **The model router is down**: Open WebUI shows an error from the
  agent; nothing else is tried.
- **A long tool call** (an approval wait, a slow Paperless upload): the
  chat stays open; the runtime's keep-alives prevent client timeouts.
- **The runtime's own update checks or hosted services** (its vendor's
  tool gateway, telemetry): unreachable by design (no internet); turned
  off in configuration where a setting exists.
- **Per-user visibility**: this feature provisions only the Household
  agent, which every household member may use (architecture, Users and
  household model). Personal agents, visible only to their owner, come
  with 012.
- **Conversation history**: Open WebUI keeps the chat; the runtime may
  keep its own session copy in its private volume. Neither is shared
  with other agents.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: sesh MUST run an adopted agent runtime for an agent, in a
  container of its own, configured by sesh.
- **FR-002**: The runtime's built-in tools and toolsets MUST be turned
  off; the only tools it offers the model MUST be the gateway's.
- **FR-003**: The runtime MUST reach the gateway's agent endpoint with
  that agent's gateway token, and nothing else holding a resource
  credential.
- **FR-004**: The runtime MUST use the model router for inference,
  through the household's agent alias (`agent-main`), with its own
  router key limited to that alias.
- **FR-005**: The agent's container MUST be on a network of its own
  shared only with the gateway, the model router and the chat UI; it
  MUST have no outbound internet access and reach nothing else.
- **FR-006**: Open WebUI MUST list the agent as a model and stream its
  answers; the connection MUST use a per-agent key that the agent's
  runtime requires.
- **FR-007**: The gateway's agent endpoint MUST be reachable on each
  agent's network and still MUST NOT be reachable on any executor,
  resource, database or pair network.
- **FR-008**: `sesh agent add … --runtime hermes` and `sesh agent
  remove` MUST provision and remove the runtime container, its network,
  its keys and its Open WebUI connection, applied by `sesh up`.
- **FR-009**: `sesh doctor` MUST check each agent runtime's health and
  that its offered tools are only the gateway's.
- **FR-010**: The runtime's image MUST be pinned by version and digest;
  its state MUST live in a per-agent volume; its container MUST run
  without privilege escalation.
- **FR-011**: The network layout (feature 008) MUST include agent
  networks; the reachability test and the doctor's layout check MUST
  cover them.
- **FR-012**: A stack test MUST run a scripted conversation through
  Open WebUI and the runtime to the gateway (weather, search, mail,
  feeds, a gated send), check the offered tools, refuse a non-gateway
  tool call, and probe the container's reachability.
- **FR-013**: The decision MUST be recorded in an ADR (a new adopted
  component, a change to what talks to what and to the gateway's
  listener rule), and the architecture document and diagram MUST
  describe it in the same change.

### Key Entities

- **Agent runtime**: an agent's running runtime: its image, its
  network, its private state volume, its gateway token, its router key
  and its API key for Open WebUI.
- **Agent network**: an internal network per agent runtime shared with
  the gateway, the model router and Open WebUI.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A household member gets an answer to a weather question
  and to a search question from the agent in Open WebUI, each using the
  gateway, in the stub-mode stack test.
- **SC-002**: The tools offered to the model are 100% the gateway's: 0
  runtime built-in tools in any request.
- **SC-003**: 0 connections succeed from the agent's container to any
  destination other than the gateway's agent endpoint, the model router
  and Open WebUI.
- **SC-004**: A mail send requested in chat produces exactly one
  approval request and 0 sends before approval.
- **SC-005**: Adding the agent is one CLI command plus `sesh up`, and
  removing it is one command plus `sesh up`; doctor passes after each.
- **SC-006**: Every existing stack test still passes.

## Assumptions

- **The agent runtime is Hermes Agent** (Nous Research, MIT), as the
  architecture names it, run from its official image.
- **Only the Household agent** is provisioned in this feature, visible
  to every Open WebUI user; per-user agents and visibility come with
  012.
- **No memory**: the runtime's memory is off; memory comes in 011.
- **No skills, messaging platforms, cron or delegation**: off until a
  later feature asks for them.
- **The model is the household's `agent-main` alias**: the runtime gets
  its own router key, limited to that alias, as Open WebUI and the eval
  have.
- **Open WebUI connects to the runtime directly** as an
  OpenAI-compatible connection, alongside the model router; Open WebUI
  stays the household's chat UI.
