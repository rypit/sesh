# Quickstart: Model evals

Prerequisites: the stack from 001 and 002 (`sesh init`, `sesh up`), the
dev images (stub, gateway, executor).

## Against the stub model (development, CI)

```bash
sesh init --stub && sesh up
STUB_SCRIPT=pass sesh up   # stub plays the passing script
sesh eval                 # every task PASS, exit 0
STUB_SCRIPT=fail sesh up   # and the failing one
sesh eval                 # expected tasks FAIL with expected reasons, exit 1
sesh eval list            # both runs
sesh eval compare         # tasks that changed, marked *
sesh eval show <run> refusal
```

## Against a real model (operator)

```bash
sesh init --inference-url http://<server>/v1 --model <name> && sesh up
sesh eval                 # verdict for this model
sesh eval --threshold 4   # accept one failure
```

Expected: the run is saved under `~/.local/share/sesh/evals/`; the
household gateway's agents and audit log are unchanged
(`sesh agent list`, the audit row count).
