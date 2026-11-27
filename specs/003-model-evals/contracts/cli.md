# Contract: `sesh eval`

```
sesh eval [--alias NAME] [--threshold N] [--turns N] [--tool-calls N] [--timeout SECS] [--json]
sesh eval list [--json]
sesh eval compare [RUN_A] [RUN_B]
sesh eval show RUN TASK
```

## `sesh eval` (run the suite)

- **Needs:** `sesh up` done (the router answers, and `EVAL_ROUTER_KEY`
  exists). Otherwise it exits 3 with "run `sesh up` first".
- **Defaults:** `--alias agent-main`, threshold = number of tasks, and
  limits 8 / 6 / 120.
- **Steps:**
  1. Takes `evals/.lock`; exits 3 if another run holds it.
  2. Starts the `sesh-eval` project and waits for health.
  3. Runs the tasks in order.
  4. Saves the run and removes the project. On Ctrl-C it removes the
     project and marks the run incomplete.
- **Output:** one line per task, then a summary:

```
web-fact   PASS  2 calls  3 turns  4.1s
forecast   FAIL  expected a geo.forecast call; none made
...
agent-main (qwen3 on http://srv:8080/v1), suite 1: 4/5 passed, threshold 5 → FAIL
saved evals/20261127T101500Z-agent-main
```

- **`--json`:** prints the run (data-model.md) instead.
- **Exit codes:** 0 when the verdict passes; 1 when it fails; 3 for a
  missing prerequisite or a run already in progress.

## `sesh eval list`

Newest first, one run per line: id, model, server, `passed/total`,
complete or incomplete.

## `sesh eval compare [A] [B]`

- **Defaults:** the latest two complete runs. With one argument, that
  run against the latest.
- **Output:** a table of task, A, B, with changed rows marked `*`.
- **Notes:** says so when the suite versions differ, and when the models
  match but the results don't (a non-deterministic model).

## `sesh eval show RUN TASK`

Prints the task's transcript under the header
`untrusted model output and tool results`, then its audit entries and
reasons.
