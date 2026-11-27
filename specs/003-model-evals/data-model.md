# Data model: Model evals

## Task (in the suite, compiled into the CLI)

| Field | Notes |
|---|---|
| `id` | e.g. `web-fact`, `forecast`, `combined`, `refusal`, `approval` |
| `prompt` | user message; starts with `[task:<id>]`; never contains `expect.facts` |
| `expect.calls` | required audit outcomes, e.g. `search.web allowed`, `testgate.act refused` |
| `expect.facts` | strings the answer must contain (matching rules: research R7) |
| `expect.sources` | URLs the answer must contain |
| `expect.not_claims` | phrases that fail the task if present (approval task: "done", "sent", "completed") |
| `expect.recovered` | refusal task: a refused call followed by an allowed one of the same method |
| `limits` | turns, tool calls, seconds; defaults 8, 6, 120 |

`SUITE_VERSION` changes whenever any task changes.

## Task result

| Field | Notes |
|---|---|
| `task` | id |
| `passed` | bool |
| `reasons` | one line per failed rule, e.g. `expected a search.web call; none made` |
| `tool_calls` | count of MCP tool calls the model made |
| `turns` | model replies |
| `seconds` | wall time |
| `calls` | audit entries for the task: connector, method, outcome, reason |
| `answer` | final answer, truncated to 4 KiB |

## Run

| Field | Notes |
|---|---|
| `id` | directory name: `<UTC yyyymmddThhmmssZ>-<alias>` |
| `alias`, `model`, `server` | label (research R8) |
| `suite_version`, `sesh_version` | |
| `threshold` | tasks that must pass; default: all |
| `results` | task results, in suite order |
| `passed` | count; `verdict` = `passed >= threshold` |
| `complete` | false until every task ran; incomplete runs are listed as such and never compared |

## Transcript (per task)

The messages sent and received, labelled `untrusted: true`: system,
user, assistant (content and tool calls) and tool results. Plus the
task's audit entries. Never read back as input to anything but
`sesh eval show`.

## Eval agent

`name = "eval"`, connectors `search, geo, testgate`. Its token is 32
random bytes, held in the CLI's memory for one run. Only its SHA-256
reaches the eval gateway's `agents.toml`, in the run's temporary
directory.
