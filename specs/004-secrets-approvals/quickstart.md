# Quickstart: Secrets and approvals

Prerequisites: the dev images (stub, gateway, executor), plus the
Infisical and ntfy images, which `sesh up` pulls.

```bash
sesh init --stub && sesh agent add alice --connectors search,geo,testgate,testcred
sesh up                                    # bootstraps Infisical and ntfy the first time
sesh credential request testcred TOKEN     # open the printed link, paste a value
sesh credential list                       # testcred TOKEN set
```

- **The approval path:** a script calling `testgate.act({})` returns
  `approval_pending`. The topic (`sesh.toml [gateway] ntfy_topic`) gets a
  "Review" notification. Open its link, then tap Approve, then re-run the
  script: it returns `{done: true}`. A second re-run asks again.
- **Your phone:**
  1. Run `tailscale serve` for ports 7002 and 8090 (ntfy).
  2. Set `approvals_base_url` to the Tailscale URL.
  3. Subscribe in the ntfy app as `person`, with the password from
     `secrets.env`.
- **Expected:** credential values never appear in `sesh` output, gateway
  logs, the audit table or script results.
