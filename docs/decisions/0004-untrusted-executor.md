# 0004. The script executor is untrusted; the gateway holds all authority

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

Agents act by writing short scripts that call connectors (Code Mode).
The scripts run in workerd with `@cloudflare/codemode`'s runtime, which
provides a durable call log, approvals by abort and replay, rollback,
snippets, search and describe. The prototype (docs/research/codemode.md)
showed that under local workerd:

- scripts have no network and no environment, as intended;
- the runtime, its log and its approval state run in the same process as
  the scripts, separated only by V8 isolates;
- CPU limits are not enforced, and all runs in one process share a
  thread, so one runaway script delays every other run.

## Decision

1. **The executor is untrusted.** Nothing it reports (approval state,
   which agent it runs for, call results it relays) is authority.
2. **The gateway (Rust) holds all authority.** It:
   - is the MCP server agents talk to, and starts each execution with a
     short-lived capability bound to the calling agent's token;
   - checks that capability on every connector call the executor
     forwards, and derives scope from it;
   - alone reads secrets, calls resources and writes the audit log;
   - runs an approval-gated method only with a one-time approval token
     from the person's ntfy approval, whatever the executor's state;
   - enforces a wall-clock deadline per execution and restarts the
     executor when it is exceeded.
3. **One executor per agent**, as its own container with no network
   except its single binding to the gateway, a memory cap and no
   secrets. A runaway or compromised script affects only its own agent.
4. **Connectors in the executor are stubs** that forward each call to
   the gateway; their tool descriptions and approval flags come from the
   gateway, which re-checks both.

## Alternatives

- **Executor inside the trusted zone**: simpler, but trusts approval
  state and scoping to a process that runs model-written code.
- **One shared executor**: fewer containers, but one user's runaway
  script stalls the household, and an isolate escape reaches every
  agent's runtime state.
- **A Rust executor runtime**: see ADR 0002.

## Consequences

- Constitution I lists the executor as untrusted.
- Negative tests: a forwarded call with a missing, expired or other
  agent's capability is refused; a gated call without an approval token
  is refused even if the executor marks it approved; a script past its
  deadline is stopped and its executor restarted.
- The memory budget adds one small executor container per agent.
