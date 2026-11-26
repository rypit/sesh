# Feature Specification: Gateway core

**Feature Branch**: `002-gateway-core`

**Created**: 2026-11-26

**Status**: Draft

**Input**: User description: "Phase 2 — Gateway core. The gateway agents talk to: Code Mode tools (execute a script, search connectors, describe a connector or method); token auth per agent mapping to an allowlist of connectors; scripts run in an untrusted per-agent executor with no network except its route to the gateway and no secrets; a short-lived capability per execution, checked on every forwarded call; scope from the token, never the script; a wall-clock deadline per execution with executor restart; approval-gated methods refused until the approval channel exists; an append-only audit log agents can't read. First connectors: search (SearXNG, Kiwix) and geo (forecast, geocoding, nearby places). Acceptance: a script calling two connectors in one execution works; the sandbox cannot read env, files or network; every call appears in the audit log; an unknown token is refused; plus ADR 0004's negative tests."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An agent gets work done by writing one script (Priority: P1)

An agent (a test client standing in for Hermes) connects to the gateway
with its own token, looks up which connectors it has, and runs one short
script that searches the web and checks the weather, getting both results
back in one step.

**Why this priority**: this is the gateway's whole purpose and the
pattern every later phase uses (constitution II, "one way out").

**Independent Test**: with the stack running against fake upstreams,
connect as a test agent, search for connectors, describe one, and run a
script that calls `search` and `geo` and returns their combined result.

**Acceptance Scenarios**:

1. **Given** an agent token granted `search` and `geo`, **When** the agent
   searches the available connectors, **Then** it sees methods of `search`
   and `geo` only, each with its description.
2. **Given** the same agent, **When** it asks for one method's
   description, **Then** it gets that method's input and output types.
3. **Given** the same agent, **When** it runs a script that calls a search
   method and a forecast method and returns both, **Then** the result
   contains both, and the script's printed output is returned too.
4. **Given** a script with a syntax error or a thrown error, **When** it
   runs, **Then** the agent gets the error message as a result, not a
   failure of the gateway.

---

### User Story 2 - A manipulated agent can't escape or widen its reach (Priority: P1)

Whatever a script tries, it can do nothing except call the connectors its
agent was granted, through the gateway, inside its own scope.

**Why this priority**: agents are assumed compromised at any moment
(constitution I); this is the trust boundary.

**Independent Test**: run hostile scripts and requests and see each
refused, with nothing leaking and every refusal recorded.

**Acceptance Scenarios**:

1. **Given** a script that reads environment variables, files, or tries
   any network connection, **When** it runs, **Then** it finds no
   environment, no files and no network.
2. **Given** a request with an unknown or missing token, **When** it
   reaches the gateway, **Then** it is refused.
3. **Given** an agent granted only `search`, **When** its script calls
   `geo`, **Then** the call is refused.
4. **Given** a forwarded connector call carrying a forged capability, an
   expired one, or one issued to another agent, **When** it reaches the
   gateway, **Then** it is refused.
5. **Given** an approval-gated method, **When** a script calls it, even
   with the executor reporting it as approved, **Then** the gateway
   refuses it (no approval channel exists yet).
6. **Given** a script that names a different user or scope in its
   arguments, **When** it calls a connector, **Then** the scope used is
   still the one derived from the agent's token.

---

### User Story 3 - Every action is on the record (Priority: P1)

Every call the gateway handles, allowed or refused, is written to an
append-only audit log that no agent can read or change.

**Why this priority**: the operator needs to see what each agent did and
what was stopped (constitution I, X).

**Independent Test**: run allowed and refused calls, then read the audit
log as the operator: one entry per call; try to update, delete or read it
as the gateway's agent-facing role: refused.

**Acceptance Scenarios**:

1. **Given** a script that made two connector calls, **When** the operator
   reads the audit log, **Then** there are two entries with agent,
   connector, method, a redacted argument summary, the outcome and time.
2. **Given** a refused call, **When** the operator reads the log, **Then**
   the entry has the refusal reason.
3. **Given** the audit store, **When** anything tries to change or delete
   an entry, **Then** it is refused; agents have no way to read it.
4. **Given** repeated refusals from one agent in a short time, **When**
   the threshold is passed, **Then** the operator is told in
   `sesh doctor` output (alerts via the approval channel come in Phase 5).

---

### User Story 4 - One agent's runaway script can't hurt the others (Priority: P2)

A script that loops forever is stopped at its deadline, its agent's
executor restarts, and other agents keep working meanwhile.

**Why this priority**: the executor enforces no CPU limits itself
(docs/research/codemode.md); without this one script stalls the
household.

**Independent Test**: start a looping script for agent A, then run a
normal script for agent B: B finishes on time; A gets a deadline error and
works again afterwards.

**Acceptance Scenarios**:

1. **Given** a script that never finishes, **When** its deadline passes,
   **Then** the agent gets a timeout error and its executor is restarted.
2. **Given** agent A's script is looping, **When** agent B runs a script,
   **Then** B's result arrives in normal time.
3. **Given** an executor was restarted, **When** its agent runs the next
   script, **Then** it works normally.

---

### User Story 5 - The operator manages agents and connectors (Priority: P2)

The operator adds and removes agents and chooses which connectors each
may use, from the command line, and checks the gateway with `sesh doctor`.

**Why this priority**: tests and later phases need agents to exist;
real per-user provisioning comes in Phase 8.

**Independent Test**: add an agent with `search` only, run a script as
it, remove the agent, and see its token refused.

**Acceptance Scenarios**:

1. **Given** the stack, **When** the operator runs
   `sesh agent add <name> --connectors search,geo`, **Then** the agent
   exists, its token is stored for the agent's container (never printed),
   and its executor starts.
2. **Given** an agent, **When** the operator removes it, **Then** its
   token is refused and its executor is gone.
3. **Given** the stack, **When** the operator runs `sesh doctor`, **Then**
   it also reports the gateway's health and each agent's executor.

### Edge Cases

- The search or weather upstream is down: the method returns a clear
  error to the script; the call is audited as failed.
- A script returns a very large result: it is truncated to a size limit
  with a note, so it can't flood the agent's context.
- A connector call's arguments don't match its declared input: refused
  before reaching the upstream, with the validation message.
- The executor crashes mid-run: the agent gets an error; the next run
  starts a fresh executor.
- The audit store is unavailable: the gateway refuses calls rather than
  run them unrecorded.
- Many calls in one script: a per-execution call limit stops loops that
  hammer upstreams.
- An agent's token is presented to the executor-facing endpoint, or a
  capability to the agent-facing endpoint: refused (the two credentials
  are not interchangeable).
- Search results contain instructions ("ignore previous instructions…"):
  returned as data and labelled untrusted; the gateway never acts on them.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The gateway MUST expose to agents a tool to run a script, a
  tool to search the connector methods available to that agent, and a
  tool to describe a connector or method, over the agent tool protocol
  the agents use (MCP).
- **FR-002**: Every agent request MUST carry the agent's token; unknown,
  missing or revoked tokens MUST be refused.
- **FR-003**: Each agent record MUST map a token to an allowlist of
  connectors; the gateway MUST expose and permit only those, and MUST
  derive all scope from the token, never from script or arguments.
- **FR-004**: Scripts MUST run in an executor dedicated to that agent,
  with no network except its single route to the gateway, no environment
  and no files, and no credentials.
- **FR-005**: For each execution the gateway MUST issue a short-lived
  capability bound to the agent and execution, and MUST check it on every
  connector call the executor forwards; forged, expired or other-agent
  capabilities MUST be refused, and agent tokens and capabilities MUST
  not be interchangeable.
- **FR-006**: The gateway MUST enforce a wall-clock deadline per execution
  (default 30 s, configurable), stop waiting at the deadline, return a
  timeout error, and restart that agent's executor.
- **FR-007**: A runaway execution for one agent MUST NOT delay executions
  of other agents beyond normal variation.
- **FR-008**: Methods marked as requiring approval MUST be refused by the
  gateway until the approval channel exists, regardless of anything the
  executor reports.
- **FR-009**: The gateway MUST validate each call's arguments against the
  method's declared input before calling the upstream.
- **FR-010**: Every call, allowed or refused, MUST be recorded in an
  append-only audit log with time, agent, connector, method, a redacted
  argument summary, the outcome and any refusal reason; entries MUST NOT
  be changeable or deletable through the gateway's credentials, and no
  agent MUST be able to read the log. If the log can't be written, the
  call MUST be refused.
- **FR-011**: The `search` connector MUST provide web search (through the
  local search service) and offline encyclopedia search and article
  retrieval; the `geo` connector MUST provide a forecast for a place,
  geocoding of a place name, and nearby places of a kind, with the
  household's home location as the default place.
- **FR-012**: All upstream addresses MUST be configurable, so tests use
  fakes and no test reaches the internet.
- **FR-013**: Content returned from upstreams MUST be marked as untrusted
  data in the result the agent sees.
- **FR-014**: Each execution MUST have limits on number of connector
  calls and on result size; exceeding them ends the execution with a
  clear error or truncates the result with a note.
- **FR-015**: The search service and encyclopedia service MUST be
  reachable only from the gateway, not from executors or agents.
- **FR-016**: The operator MUST be able to add and remove agents with
  their connector allowlists, and `sesh doctor` MUST report the gateway's
  health, each agent's executor, and repeated refusals per agent.
- **FR-017**: Negative tests MUST cover each refusal in User Story 2, the
  audit log's immutability, the deadline and restart, and cross-agent
  isolation.

### Key Entities

- **Agent**: a name, a token (stored hashed by the gateway; the plain
  token only in the agent's container environment), an allowlist of
  connectors, and an executor.
- **Connector**: a namespace of methods (e.g. `search`, `geo`); each
  method has a description, declared input and output, and whether it
  requires approval.
- **Execution**: one run of one script for one agent, with an id, a
  capability, a deadline, call and size limits, and an outcome.
- **Capability**: a short-lived credential bound to one agent and one
  execution, held by the executor, accepted only on the executor-facing
  side.
- **Audit entry**: time, agent, execution, connector, method, redacted
  argument summary, outcome (allowed, refused, failed), reason.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A script making two connector calls against local fakes
  completes in under 2 seconds.
- **SC-002**: 100% of the hostile cases in User Story 2 are refused, and
  each refusal appears in the audit log.
- **SC-003**: 100% of calls (allowed and refused) in the test suite have
  exactly one audit entry.
- **SC-004**: A runaway script is stopped within 5 seconds after its
  deadline, and another agent's script started meanwhile finishes within
  2 seconds.
- **SC-005**: No test reaches the internet; the suite passes with all
  outbound internet blocked.
- **SC-006**: No secret value (agent token, capability, database
  password) appears in the audit log, gateway logs or any result returned
  to an agent.

## Assumptions

- A test client speaking the agent tool protocol stands in for Hermes,
  which arrives in Phase 4 (one agent end to end).
- Agents are defined in operator configuration for now; full per-user
  provisioning (`sesh user add`) is Phase 8.
- No connector in this phase has side effects, so approvals are
  exercised with a test-only gated method that the gateway must refuse.
- Agents' and executors' containers join the stack; agents reach only the
  gateway, executors reach only the gateway, and the gateway reaches the
  search services and the configured upstreams.
- Weather and maps use public, keyless services (Open-Meteo,
  OpenStreetMap) in real use; the household's home location is set by
  the operator.
- The offline encyclopedia starts with a small sample archive in tests;
  the full English Wikipedia archive (~50 GB) is an operator download.
- Repeated-refusal alerts are reported by `sesh doctor` now and pushed to
  the approval channel when it exists (Phase 5).
- Default limits: 30 s deadline, 50 connector calls per execution, 64 KB
  result; all configurable.
