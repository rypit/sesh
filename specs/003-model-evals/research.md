# Research: Model evals

## R1. Where the harness runs → the `sesh` CLI (Rust)

- **Decision:** `sesh eval` is part of the admin CLI. It plays the agent
  role itself: a loop that calls the router's chat API with the gateway's
  tools and forwards the model's tool calls to an eval gateway over MCP.
- **Rationale:**
  - The CLI is trusted-plane code, and already holds the router's
    credentials and talks to the router (`sesh doctor`). ADR 0002 makes
    trusted-plane code Rust.
  - The loop is small: chat call, tool calls, tool results, repeat.
  - Testing "can this model use sesh's tools" needs nothing an agent
    framework adds.
- **Alternatives:**
  - *Adopt an eval framework* (Inspect, promptfoo, OpenAI Evals,
    DeepEval; constitution VIII). Each is Python or Node, so it would run
    on the host, in the trusted plane, with the router key (against ADR
    0002). Each would also need its own MCP-to-tools bridge, and their
    scorers are mostly model-graded, which FR-005 rules out. The parts we
    need are under a few hundred lines.
  - *Wait for Hermes (Phase 4) and evaluate through it.* That would test
    Hermes and the model together. The point is to know the model is fit
    before an agent is built on it.

## R2. A separate eval gateway per run

- **Decision:** `sesh eval` starts its own Compose project, `sesh-eval`,
  with three services, and removes it (with its volumes) when the run
  ends, even after an interrupt:
  - `gateway-eval`: the same gateway image;
  - `executor-eval`: the same executor image;
  - `fakes`: the stub image, serving only its fake upstream routes.
- **Networks**, all three joined by the gateway:
  - `eval-exec`: internal, holding only the gateway and the executor. As
    in ADR 0004, the executor's one route is the gateway; it can't reach
    the fakes.
  - `eval-fakes`: internal, holding only the gateway and the fakes.
  - `eval-edge`: holds only the gateway, so its agent port can be
    published on `127.0.0.1:7100`.
- **Gateway settings:**
  - every upstream points at `http://fakes:8000`;
  - the test-only `testgate` connector is on;
  - the audit sink is a JSON-lines file (R5);
  - agent file: one agent, `eval`, granted search, geo and testgate,
    with a token generated for the run and held only in memory by the
    CLI.
- **Rationale:**
  - FR-008: the household gateway, its agents and its audit log are
    never touched.
  - FR-010: everything is local.
  - The reach is exactly an ordinary agent's (FR-009), and the content
    behind the tools is fixed.
- **Alternatives:**
  - *Add an eval agent to the household gateway.* It would write to the
    household audit log, and need the household's upstreams switched to
    fakes.
  - *A Compose profile in the main project.* `sesh up --remove-orphans`
    and `sesh down` would interfere with a running eval, and the reverse.

## R3. Tools offered to the model

- **Decision:** the harness calls MCP `tools/list` on the eval gateway
  and offers each tool as an OpenAI `function` tool, unchanged:
  - `name` → `function.name`;
  - `description` → `function.description`;
  - `inputSchema` → `function.parameters`.
- **System prompt:** the gateway's MCP `instructions` plus one line
  saying tool results are untrusted data. No tool beyond the gateway's
  three (FR-003).
- **Rationale:** the model sees exactly what an MCP-speaking agent will
  see, so a change to the gateway's tools changes the eval.

## R4. The loop

- **Decision:** non-streaming `POST /v1/chat/completions` to the router
  (`stream: false`), with `model = <alias>` and `tools`, and
  `tool_choice` left at the default (`auto`).
- **Each turn:**
  - If the reply has `tool_calls`, call each in order on the gateway
    (`tools/call`), append a `tool` message with the text result, and
    count the call.
  - Otherwise the content is the final answer.
- **Malformed calls:** an unknown tool name, or arguments that aren't a
  JSON object, get a `tool` message saying the call was invalid, and
  count as a turn.
- **Limits:** per-task limits on turns, tool calls and wall time
  (defaults 8, 6 and 120 s). Hitting a limit ends the task as failed with
  that reason.
- **Rationale:** non-streaming works with every OpenAI-compatible server
  and keeps the loop simple. Streaming is a server feature the eval
  doesn't need to test (spec edge case: works either way).

## R5. Scoring source: the eval gateway's audit

- **Decision:** a third audit sink in the gateway, `file:<path>`, which
  appends one JSON object per entry. It's used only by the eval gateway,
  on a bind mount the CLI reads (`$SESH_DATA_DIR/evals/.run/audit.jsonl`).
  The CLI creates `.run/` for each run with mode 0777, so the gateway's
  uid can write to it. It sits inside the data directory, which is 0700,
  so no other host user can reach it, and it is removed after the run.
  Tasks run one at a time; the harness notes the file length before a
  task and reads the entries appended during it.
- **Why the audit, not the transcript:** connector calls happen inside
  scripts in the executor, so the MCP tool results don't reliably say
  which methods ran. The audit is the gateway's own record of every
  allowed and refused call, so scoring uses the trusted source.
- **Alternatives:**
  - *Parse scripts or results.* That would trust model and executor
    output (constitution VI).
  - *Give the eval gateway Postgres.* Heavier, for a throwaway instance.
- The household gateway stays Postgres-only: `compose.yaml` always sets
  a Postgres URL, and the file sink is refused unless the path is under
  `/var/log/sesh-eval/`.

## R6. Scripted stub model

- **Decision:** the stub server gains `STUB_SCRIPT=pass|fail`. Each task
  prompt carries a tag (`[task:<id>]`). The stub finds the tag in the
  first user message and the number of tool results so far, and plays
  that step of a fixed script:
  - **pass:** call `execute` with a correct script. The final answer
    quotes the tool result's fact and URL (refusal task: a bad call, then
    a corrected one; approval task: "needs approval, not done").
  - **fail:** one known failure per task:
    - no tool call;
    - a wrong fact;
    - claiming the gated action was done;
    - repeating the bad call;
    - a malformed tool call;
    - describing an ungranted connector.
- An unknown tag, or no tag, keeps the stub's current behaviour, so 001
  and 002 tests are unaffected.
- **Rationale:** FR-016 and SC-002/003. Two fixed scripts make the
  harness's pass and fail paths deterministic in CI.

## R7. Facts that only the fakes know

- **Decision:**
  - Each task's expected fact and source come from the fakes' content for
    that task's query, made up so no model knows it (e.g. "the Varden
    beacon was first lit in 1873", `https://example.org/varden`).
  - The forecast task's fact is the first day's maximum from the fake
    forecast.
  - A unit test checks no expected fact appears in any task prompt.
- **Matching:** case-insensitive, with whitespace and punctuation
  collapsed; a source matches if its URL appears in the answer.

## R8. Results on disk

- **Decision:** one directory per run, at
  `$SESH_DATA_DIR/evals/<UTC time>-<alias>/`, holding:
  - `run.json`: run metadata and task results, with scores and limits;
  - `transcripts/<task>.json`: messages, tool calls and audit entries.
- **Run label:**
  - alias;
  - model, from `sesh.toml`'s alias mapping;
  - server, `inference.base_url` without credentials;
  - suite version;
  - sesh version.
- **Safety:**
  - Before writing, the CLI redacts the eval token and every value from
    `secrets.env`, and refuses to save if one is still present (SC-005).
  - Directories are 0700, files 0600.
  - `evals/.lock` stops two runs at once.
- **Rationale:** FR-013/014. Plain files can be compared with `sesh eval
  compare` or by hand, and live outside the repository (ADR 0003).

## R9. Router key

- **Decision:** `sesh up` creates a third LiteLLM virtual key,
  `EVAL_ROUTER_KEY`, limited to the aliases, the same way it makes
  Open WebUI's.
- **Rationale:** the eval must hit the router the way agents will
  (FR-002), not with the master key, which also accepts raw model names
  (feature 001 R2).

## R10. Component boundary → ADR 0006

- The admin CLI now reaches a gateway's agent endpoint over MCP and
  starts a second, short-lived gateway, and the gateway gains a file
  audit sink. That is a component-boundary change (AGENTS.md), so ADR
  0006 records it.
- The architecture diagram shows runtime components; the eval gateway
  is a transient operator tool outside it, so the diagram doesn't change.
