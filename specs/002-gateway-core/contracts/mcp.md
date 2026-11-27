# Contract: agent-facing MCP tools (`http://gateway:7000/mcp`)

Auth: `Authorization: Bearer <agent token>`; unknown or missing → HTTP 401.

| Tool | Input | Result |
|---|---|---|
| `execute` | `{ code: string }` — an async arrow function, e.g. `async () => { const w = await geo.forecast({}); return w; }` | text: a header line saying the content is untrusted data, then JSON `{status: "completed" \| "error" \| "timeout" \| "approval_unavailable", result?, error?, logs?}`, truncated to `max_result_bytes` with a note |
| `search` | `{ query: string }` | the agent's connector methods ranked by match: `{path, description, requires_approval}` |
| `describe` | `{ target: string }` — `"geo"` or `"geo.forecast"` | TypeScript declarations and descriptions |

Only the agent's allowlisted connectors appear in `search`, `describe`
and the sandbox.
