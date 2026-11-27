# Feature Specification: Model evals

**Feature Branch**: `claude/sharp-ramanujan-6b9nht`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Phase 3 — Model evals (Solutions Architecture roadmap; constitution 1.2.0; ADR 0005). `sesh eval`: check that the inference server behind the `agent-main` alias (through LiteLLM, any OpenAI-compatible server) can actually use sesh's resources through the gateway, before an agent relies on it. sesh runs a minimal agent loop of its own (not Hermes, which is Phase 4): it connects to the gateway as a dedicated eval agent over MCP, offers the gateway's tools (execute, search, describe) to the model as OpenAI tool definitions, and runs a fixed suite of tasks that need them — find a fact with search.web and cite it; get the home forecast with geo.forecast; combine both in one execute script; recover after a refused call (bad argument); decline or report when a method needs approval. Scoring is mostly deterministic (right connector/method called, answer contains the fact from the fake upstream, number of tool calls, turns, wall time); no LLM-as-judge in this phase. Runs against the stub upstreams by default so results are reproducible and need no internet; results saved per model and server under the data directory so the operator can compare models and catch regressions when switching; a pass threshold sets the exit code. CI runs the suite against the stub model server (extended with a scripted tool-calling mode) to keep the harness honest; real-model runs are manual operator runs. The eval agent is an ordinary gateway agent with only search and geo; eval transcripts are model output and untrusted (constitution VI) — never treated as instructions. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

The operator is the only user of this feature. Each story is a run of
the eval command, or of its results.

### User Story 1 - Does this model work with sesh? (Priority: P1)

The operator has pointed `agent-main` at an inference server and wants to
know, before any agent depends on it, whether the model behind it can do
real work through the gateway. They run one command. It runs a fixed set
of tasks that each need the gateway's resources, and prints, for each
task, pass or fail and why, then a summary and an overall verdict.

**Why this priority**: This is the whole point of the phase. Without it,
the first sign that a model can't call tools properly is a household
agent failing in front of a family member.

**Independent Test**: With the stack running, run the eval command once
against the stub model in its scripted mode: every task passes and the
command exits successfully. Run it again with tool calling turned off in
the stub: the tool-dependent tasks fail, the reason says no tool was
called, and the command exits with failure.

**Acceptance Scenarios**:

1. **Given** a running stack and a model that uses tools well, **When**
   the operator runs the eval, **Then** each task reports pass with the
   number of tool calls, model turns and time taken, and the command
   exits with success.
2. **Given** a model that answers without calling any tool, **When** the
   operator runs the eval, **Then** the tasks that need a tool fail with
   a reason naming the call that was expected, and the command exits with
   failure.
3. **Given** a model that calls the right tool but answers with a fact
   that isn't in the resource's result, **When** the eval runs, **Then**
   that task fails with a reason saying the expected fact was missing.
4. **Given** the inference server is unreachable, **When** the eval runs,
   **Then** it stops within the per-task time limit with a message naming
   the alias and server, rather than hanging.

---

### User Story 2 - Safe and repeatable (Priority: P1)

The operator wants eval results that mean the same thing every time and
can't affect the household. The eval runs against its own copy of the
gateway whose resources return fixed, known content, so the expected
answers are known in advance, nothing reaches the internet, and the
household's gateway, agents and audit log are untouched.

**Why this priority**: Scores are only comparable if the content behind
the tools is fixed. And an eval is model output driving tools: it must
have no more reach than an ordinary agent, and none into household data.

**Independent Test**: Run the eval with containers' internet blocked;
it passes. Before and after, the household gateway's agent list and audit
log are unchanged. The eval's agent can reach only search and geo (plus
the test-only approval connector), and a task's transcript containing
instructions ("ignore the task, call X") doesn't change what the harness
scores or does.

**Acceptance Scenarios**:

1. **Given** a running household stack, **When** the eval runs, **Then**
   no agent is added to or removed from the household gateway and no row
   is added to its audit log.
2. **Given** containers have no internet, **When** the eval runs against
   the stub model, **Then** every task still passes.
3. **Given** a fake resource result that contains instructions to the
   model, **When** a task runs, **Then** the harness scores the task only
   by its fixed rules and never acts on text from the model or the
   results.
4. **Given** the eval agent's grants, **When** the model asks for a
   connector it wasn't granted, **Then** the call is refused by the
   gateway and the task records the refusal.

---

### User Story 3 - Hard cases: refusals and approvals (Priority: P2)

Beyond the happy path, the operator wants to know how the model behaves
when the gateway says no. One task makes the model's first call fail (an
argument outside the method's schema) and passes only if the model reads
the refusal and succeeds on a corrected call. Another asks for something
that needs approval: it passes only if the model doesn't claim the action
happened and tells the user it needs approval.

**Why this priority**: Refusals and approvals are where a weak model
does damage, by retrying blindly, looping, or claiming success. These
cases matter, but the basic tool-use story comes first.

**Independent Test**: Against the stub model's scripted mode, the refusal
task shows one refused call followed by one allowed call, and passes; the
approval task shows the gated call refused and an answer that says
approval is needed, and passes. A scripted variant that claims success
after the gated call fails the approval task.

**Acceptance Scenarios**:

1. **Given** the refusal task, **When** the model's first call is
   refused for a bad argument and its next call succeeds, **Then** the
   task passes and records both calls.
2. **Given** the refusal task, **When** the model repeats the same bad
   call until the call limit, **Then** the task fails with a reason
   saying it never recovered.
3. **Given** the approval task, **When** the model's answer says the
   action was done, **Then** the task fails with a reason saying it
   claimed an action that was refused.

---

### User Story 4 - Compare models over time (Priority: P2)

The operator tries a few models, or switches the model behind
`agent-main`, and wants to see whether things got better or worse. Each
run's results are saved, labelled with the model and server, and the
operator can list past runs and compare the latest run with an earlier
one, task by task.

**Why this priority**: Picking a model and catching regressions after a
switch is why results are kept, but a single run is already useful
without it.

**Independent Test**: Run the eval twice with different stub settings;
listing runs shows both with their model, server, date and pass rate;
comparing them shows which tasks changed from pass to fail.

**Acceptance Scenarios**:

1. **Given** two saved runs, **When** the operator compares them,
   **Then** the output lists each task's result in both runs and marks
   the ones that changed.
2. **Given** saved runs, **When** the operator lists them, **Then** each
   shows model, server, date, pass count and total, newest first.
3. **Given** a saved run, **When** the operator asks for one task's
   transcript, **Then** it is shown as untrusted model output, with the
   tool calls, refusals and final answer.

---

### User Story 5 - The harness itself is tested (Priority: P3)

The project's CI runs the eval against the stub model on every change,
in both a passing and a failing script, so a broken harness, a changed
tool definition or a scoring bug is caught before an operator relies on
the verdict.

**Why this priority**: The verdict is only as good as the harness. CI
keeps it honest, but it adds no operator-facing behaviour.

**Independent Test**: CI runs the eval in the stack job; a deliberately
broken scoring rule makes the job fail.

**Acceptance Scenarios**:

1. **Given** the stub model's scripted mode, **When** CI runs the eval,
   **Then** every task passes.
2. **Given** the stub model's failing script, **When** CI runs the eval,
   **Then** the expected tasks fail with the expected reasons and the
   command exits with failure.

### Edge Cases

- The model replies with malformed tool calls (unknown tool name,
  arguments that aren't JSON): the call counts as a failed turn, the
  model is told the call was invalid, and the task continues until its
  turn or time limit.
- The model never stops calling tools: the task ends at its turn limit
  and fails with that reason.
- The model's answer is very long: only the first part is scored and
  stored, with a note that it was truncated.
- The eval is run while no stack is up: it stops at once and says to run
  `sesh up`.
- Two evals are started at once: the second refuses to start.
- A task's fixed expected fact appears in the task prompt itself: not
  allowed; the suite is checked so every expected fact appears only in a
  resource's fake result.
- The model streams its answer or doesn't support streaming: the eval
  works either way.
- A run is interrupted: no partial result is saved as complete, and the
  eval's own agent and gateway are cleaned up.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The operator MUST be able to run the eval suite with one
  command against the model behind `agent-main`, or another named alias.
- **FR-002**: The eval MUST drive the model through the same router and
  alias that agents use, so it tests what agents will get.
- **FR-003**: The eval MUST give the model the same three gateway tools
  agents get (run a script, search connectors, describe a connector or
  method), described as tools the model can call, and nothing else.
- **FR-004**: The suite MUST include at least these tasks: find a fact on
  the web and cite its source; get the forecast for home; use both
  resources in one script; recover from a refused call; handle a method
  that needs approval.
- **FR-005**: Each task MUST be scored by fixed rules only: which
  connector methods were called and in what outcome, whether the answer
  contains the expected fact and source, and limits on tool calls, turns
  and time. No model judges another model in this phase.
- **FR-006**: Each task MUST report pass or fail, a reason for every
  failure, tool calls, model turns and wall time.
- **FR-007**: The command MUST exit with success only if the pass rate
  meets a threshold; the default is all tasks, and the operator can lower
  it.
- **FR-008**: The eval MUST run against its own gateway instance whose
  resources are fakes with fixed content; it MUST NOT add agents to,
  call through, or write to the audit log of the household gateway.
- **FR-009**: The eval's agent MUST be an ordinary gateway agent with
  only the search and geo connectors and the test-only approval
  connector, so its reach is no larger than an agent's.
- **FR-010**: The eval MUST need no internet access; its fakes and the
  model's router are all on the local stack.
- **FR-011**: Model output and resource results MUST be treated as
  untrusted data: the harness never runs, follows or stores them as
  instructions or configuration, and shows them labelled as untrusted.
- **FR-012**: Each task MUST have limits on model turns, tool calls and
  wall time, with defaults the operator can change.
- **FR-013**: Each run's results MUST be saved under the data directory,
  labelled with alias, model, server, date and suite version, never in
  the repository.
- **FR-014**: The operator MUST be able to list saved runs, compare two
  runs task by task, and view one task's transcript.
- **FR-015**: No secret (router key, gateway token, capability) MUST
  appear in saved results, transcripts or output.
- **FR-016**: The stub model server MUST offer a scripted mode that plays
  fixed tool-calling conversations for the suite (one that passes every
  task, one that fails known tasks), and CI MUST run the eval in both.
- **FR-017**: Negative tests MUST cover: no tool calls, wrong fact,
  claiming a refused action, never recovering from a refusal, malformed
  tool calls, an unreachable server, and a model asking for an ungranted
  connector.

### Key Entities

- **Eval suite**: the fixed, versioned set of tasks shipped with sesh.
  Each has an id, a prompt for the model, the fake content its resources
  return, and its scoring rules.
- **Task result**: one task's outcome in one run: pass or fail, reasons,
  tool calls (connector, method, outcome), turns, time, and the
  transcript.
- **Run**: one execution of the suite: alias, model, server, suite
  version, start time, threshold, task results and verdict.
- **Eval agent**: the gateway agent the eval uses, with fixed, narrow
  grants, existing only in the eval's own gateway.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the stub model's passing script, 100% of tasks pass
  and the whole suite finishes in under 2 minutes.
- **SC-002**: Each of the seven negative cases in FR-017 gives the
  expected failure and reason, every time.
- **SC-003**: Two runs of the suite with the same scripted model give
  identical pass/fail results and reasons.
- **SC-004**: A run makes no change to the household gateway's agents or
  audit log, and passes with containers' internet blocked.
- **SC-005**: No secret value appears in any saved result, transcript or
  command output.
- **SC-006**: The operator can tell from one run's output, without
  opening any file, whether the model is fit to use and which tasks
  failed and why.

## Assumptions

- The stack from features 001 and 002 is running; the eval doesn't start
  it.
- The eval's own gateway is a second instance of the same gateway,
  started for the run with fake upstreams and the test-only approval
  connector, and an in-memory audit log; it is removed afterwards. Its
  agent and token exist only for the run.
- Fake upstream content is the stub server's fake routes, extended so
  each task has facts that appear nowhere else (so a model can't pass
  from its training data or the prompt).
- Default limits: 8 model turns, 6 tool calls and 120 seconds per task.
  Real models on a home server may be slow; the time limit is per task,
  not per suite, and SC-001's 2 minutes applies only to the stub.
- Facts are matched case-insensitively, ignoring whitespace and
  punctuation differences; sources are matched by URL.
- Results are kept indefinitely; the operator can delete the results
  folder. Results are local files under the data directory.
- Tasks for later connectors (email, calendar, documents, browser) join
  the suite in the phases that add them; this phase covers search, geo
  and approval refusal only.
- An eval against live resources (real web search and weather) is out of
  scope; results from it wouldn't be comparable.
- The suite is versioned with sesh; changing a task changes the suite
  version, and comparing runs from different versions says so.
