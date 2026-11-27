# Contract: CLI additions

```
sesh feed add <owner> <url> [--name <name>]   # owner: a user, or household
sesh feed remove <owner> <name>
sesh feed list [<owner>]                       # owner, name, url, who added it, last fetch
```

- They need the stack running: they call the gateway, which checks,
  stores and audits the change. `add` refuses a non-http(s) address, an
  internal address, an unknown owner, a name already used by that
  owner, and an owner at the limit, each with the reason.
- Exit codes as in feature 001: 0 success, 1 refused or failed.

`sesh doctor` gains `feeds`:
- pass: the number of feeds per owner;
- warn: a feed whose last fetch failed (named, with the reason);
- fail: `feeds_allow_hosts` is set outside stub mode.

Admin endpoints behind them (approvals listener, admin token):

| Endpoint | Body | Reply |
|---|---|---|
| `POST /admin/feeds/add` | `{owner, url, name?}` | `200 {owner, name, url}`; `404` unknown owner; `422 {error}` refused |
| `POST /admin/feeds/remove` | `{owner, name}` | `200`; `404` no such feed |
| `GET /admin/feeds` | | `{feeds: [{owner, name, url, added_by, added_at, last: {state, at?, message?}}], allow_hosts}` (the hosts the gateway honours) |
