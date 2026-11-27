# Contract: gateway file audit sink (eval only)

`SESH_AUDIT_DATABASE_URL=file:/var/log/sesh-eval/audit.jsonl`

- Writes one JSON object per line, with the same fields as the audit
  table: `at`, `agent`, `execution`, `connector`, `method`,
  `args_summary`, `outcome`, `reason`.
- Appends and flushes before the call returns, with the same
  failure-withholds-result rule as Postgres.
- Refused at start for any path outside `/var/log/sesh-eval/`.
- Only `deploy/eval/compose.yaml` sets it; `deploy/compose.yaml` always
  uses Postgres.
