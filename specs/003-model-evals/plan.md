# Implementation Plan: Model evals

**Branch**: `claude/sharp-ramanujan-6b9nht` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/003-model-evals/spec.md`

## Summary

`sesh eval` runs a fixed suite of five tasks against a router alias
(default `agent-main`). It shows whether the model behind it can use
sesh's resources through the gateway.

- **The loop:** the admin CLI runs a small agent loop of its own. It
  offers the model the gateway's three MCP tools as OpenAI tools,
  forwards the model's tool calls to the gateway, and scores each task
  by fixed rules.
- **The scoring source:** the eval gateway's own audit of which
  connector methods ran and how they ended, plus the final answer.
- **Isolation:** each run gets a separate, short-lived Compose project
  with a gateway, an executor and fake upstreams. The household stack
  is untouched and no internet is needed.
- **Results:** saved per run under the data directory, which the
  operator can list, compare and show.
- **CI:** the stub model gains a scripted mode, so CI runs the harness
  both ways: the passing script and the failing script.

## Technical Context

**Language/Version**: Rust 1.97 (trusted plane, ADR 0002)

**Primary Dependencies**:
- Existing crates only: `ureq` (router and MCP over HTTP), `serde_json`,
  `clap`, `sha2`, `getrandom`.
- No new crates: run timestamps come from `std::time::SystemTime`, formatted as UTC in code.

**Storage**: JSON files under `$SESH_DATA_DIR/evals/`

**Testing**:
- `cargo test` (unit), with fake router and fake gateway seams.
- Stack test in `crates/sesh-cli/tests/stack.rs`, run in CI with
  containers' internet blocked.

**Target Platform**: wherever `sesh` runs (Linux, macOS), with Docker

**Project Type**: CLI command plus Compose overlay

**Performance Goals**: the stub suite runs in under 2 minutes (SC-001)

**Constraints**:
- No internet.
- No writes to the household gateway or its audit log.
- No secrets in output.
- Deterministic against scripted models.

**Scale/Scope**: five tasks; one run at a time

## Constitution Check

| Principle | How this feature complies |
|---|---|
| I. Control plane is the only trusted zone | The harness is in the admin CLI (Rust). Model output only reaches the untrusted executor, through the eval gateway, as with any agent. |
| II. One way out | The model gets exactly the gateway's three tools. The eval executor's network holds only the gateway and the fakes. |
| III. Agents never see secrets | The model sees no key or token. The eval token stays in CLI memory, and saved results are redacted and checked (SC-005). |
| IV. Per-user isolation | The eval agent exists only in the eval gateway, and can't see household data. |
| V. Side effects need approval | The approval task checks the model handles the refusal. The gateway still refuses gated calls whatever the model says. |
| VI. Agent content never trusted | Transcripts are stored and shown labelled untrusted, and never fed back as instructions or configuration. Scoring uses the gateway's audit. |
| VII. Local-first | Everything runs on the local stack; no internet needed. |
| VIII. Adopt before build | Eval frameworks were considered and rejected (research R1): wrong language for the trusted plane, model-graded scoring, and their own tool bridges. The loop is small. |
| IX. Swappable inference | It runs through the alias and router, never an engine. |
| X. Every guarantee is executable | CI runs both scripts. Negative tests cover FR-017. The household audit and agents are checked unchanged. |

**Component boundary:** the CLI now talks to a gateway's MCP endpoint,
starts a second gateway, and the gateway gains a file audit sink. This
gets **ADR 0006** (`docs/decisions/0006-model-evals.md`). The
architecture diagram is unchanged (research R10).

Gate: **pass**. Re-checked after design: no new violations.

## Project Structure

### Documentation (this feature)

```text
specs/003-model-evals/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli.md
│   ├── stub-script.md
│   └── gateway-audit-file.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/sesh-cli/src/
├── eval/
│   ├── mod.rs          # `sesh eval` run/list/compare/show; lock; teardown
│   ├── suite.rs        # the five tasks, SUITE_VERSION, facts-not-in-prompts test
│   ├── run_loop.rs     # chat ↔ tool-call loop with limits (Router + Mcp seams)
│   ├── score.rs        # fixed rules → pass/fail + reasons
│   └── store.rs        # run directories, redaction, list/compare
├── host/
│   ├── mcp.rs          # seam: MCP Streamable HTTP client (initialize, tools/list, tools/call)
│   └── fake.rs         # + FakeMcp, scripted FakeRouter
├── up.rs               # + EVAL_ROUTER_KEY
└── main.rs             # + Eval subcommand
crates/sesh-gateway/src/audit.rs   # + File sink (eval only)
crates/sesh-stub-llm/src/main.rs   # + STUB_SCRIPT scripted conversations; task-specific fake facts
deploy/eval/compose.yaml           # sesh-eval project: gateway-eval, executor-eval, fakes
deploy/compose.stub.yaml           # + STUB_SCRIPT passthrough
docs/decisions/0006-model-evals.md
.github/workflows/ci.yml           # stack job unchanged; the stack test runs the eval
```

**Structure Decision**: The harness is a CLI module; nothing new runs on
the household stack. MCP gets its own seam (`host/mcp.rs`), so the
gateway's agent endpoint is reached through one module, like Compose and
HTTP.

## Complexity Tracking

None.
