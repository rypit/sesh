# Contract: gateway ↔ executor

## Gateway → executor (`POST http://executor-<agent>:8000/execute`)

```json
{ "execution": "exec_…", "capability": "…", "deadline_ms": 30000,
  "code": "async () => …",
  "connectors": [ { "name": "geo", "instructions": "…",
     "methods": { "forecast": { "description": "…", "input": { … }, "requires_approval": false } } } ] }
```

Reply: `{status: "completed", result, logs}`, `{status: "error", error,
logs}`, or `{status: "paused", pending}` (gated call). On deadline the
supervisor kills and restarts workerd and answers 504.

## Executor → gateway (`POST http://gateway:7001/call`)

Header `X-Sesh-Capability: <capability>`; body
`{connector, method, args}`. Answers `{ok: true, result}` or
`{ok: false, error}`; HTTP 403 for capability, allowlist or approval
refusals. Agent tokens are refused here; capabilities are refused on
`:7000`.
