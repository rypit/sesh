# Code Mode under local workerd: prototype findings

Prototype run on 2026-11-26 in a Linux container with
`@cloudflare/codemode` 0.5.6 (marked experimental) and `workerd`
1.20261125.1 from npm. A host Worker ran the codemode **runtime**
(`createCodemodeRuntime` + `DynamicWorkerExecutor`) in a Durable Object
with SQLite storage on local disk, plus one demo connector with a read
tool (`weather`) and an approval-gated tool (`sendEmail`). Model-written
scripts were posted to it over HTTP. No Cloudflare account was used. The
prototype code is not in this repository.

## Results

| Question | Result |
|---|---|
| Does the durable runtime run outside Cloudflare? | **Yes.** Local workerd supports Worker Loader (dynamic workers) and Durable Object facets; the runtime ran unmodified. |
| Two connector calls in one script | **Works.** Every call is recorded in the runtime's log with arguments, result and state. |
| Script can't use the network | **Holds.** `fetch()` to the internet and to `127.0.0.1`, and `cloudflare:sockets` `connect()`, all fail: "This worker is not permitted to access the internet". |
| Script can't see secrets or environment | **Holds.** `env` is undefined, `process.env` is empty; the only callable surface is `codemode.*` and the connector globals. |
| Approval-gated call pauses the run | **Works.** The run returns `status: "paused"` with the pending call; nothing executes. |
| Approve, then resume | **Works.** `approve()` re-runs the script; earlier calls are replayed from the log (not re-executed) and the approved call runs for real. |
| Search and describe | **Works.** `search("email")` returns `demo.sendEmail` with `requiresApproval: true`. |
| CPU or time limit on a runaway script | **Not enforced locally.** A synchronous loop ran 70 s past the runtime's 60 s timeout. A dynamic worker given `limits: { cpuMs: 200 }` ran a 5 s loop to completion: open-source workerd accepts the setting but doesn't enforce it. |
| Isolation between concurrent runs | **None for CPU.** One workerd process runs isolates on one thread: two overlapping 5 s scripts finished at 5.0 s and 9.8 s. A runaway script stalls every other run in that process. |

## What the library provides

More than glue. Besides generating TypeScript declarations for
connectors and bridging calls from the sandbox to the host, the
**runtime** provides:

- a durable tool-call log (one row per call, in Durable Object SQLite);
- approvals by **abort and replay**: pause at a gated call, resume later
  by re-running the script against the log, with divergence detection;
- rollback through per-tool `revert`;
- snippets (saved scripts the model can run by name);
- an execution history with retention;
- on-demand `search` and `describe`, so connector types reach the model
  only when it asks for them;
- MCP-host helpers (`runtime.execute`, `runtime.search`).

Rebuilding this in Rust (log, replay, divergence detection, approvals,
rollback, search ranking, type generation) would be a substantial
project, not a week of glue.

## Security observations

1. **Untrusted code and the runtime share a process.** Model-written
   scripts run in V8 isolates inside the same workerd process as the
   runtime and its log. On Cloudflare, other layers back the isolate
   boundary; locally, the isolate is the only boundary. Anything in that
   process, including approval state, must be treated as reachable by a
   successful isolate escape.
2. **No local CPU limits.** Time limits must be enforced from outside
   workerd: the gateway stops waiting at a deadline and restarts the
   executor.
3. **Shared thread.** Concurrent runs for different users in one
   executor process delay each other, so one user's runaway script is a
   denial of service for the whole household.

## Recommendation

Adopt codemode **inside the executor**, and keep authority in the Rust
gateway:

- **Executor (workerd container, no network, no secrets):** runs the
  codemode runtime unchanged (log, replay, search, describe, snippets).
  Its connectors are thin stubs that forward each call to the gateway
  over the executor's only outbound binding.
- **Gateway (Rust):** the MCP server agents talk to. It mints a
  short-lived capability per execution, bound to the agent's token, and
  checks it on every forwarded call. It applies scope, holds secrets,
  writes the audit log, and **independently enforces approvals**: a
  gated method runs only with a one-time approval token from ntfy,
  whatever the executor's state says. It enforces wall-clock deadlines
  and restarts the executor on timeout.
- **One executor process per agent**, so a runaway script affects only
  its own agent, and a successful isolate escape reaches only that
  agent's runtime state.

This keeps the rule that trusted-plane code is Rust, and Principle VIII
(adopt before build). Adopted in ADR 0002 and ADR 0004; constitution I
counts the executor as untrusted.

## Still open

- Memory limits per executor (container memory cap) and restart cost.
- Whether `@cloudflare/codemode`'s experimental API is stable enough to
  pin; it is at 0.5.x.
- How the gateway's MCP surface maps onto `runtime.execute` /
  `runtime.search` / `runtime.describe` for Hermes.
