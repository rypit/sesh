# Contract: CLI additions

```
sesh feed add <owner> <url> [--name <name>]   # owner: a user, or household
sesh feed remove <owner> <name>
sesh feed list [<owner>]                       # owner, name, url, last update
```

- They need the stack running: they call the gateway, which checks the
  address, creates or deletes the feed in the owner's Miniflux account,
  and audits the change. `add` refuses a non-http(s) address, an
  address with credentials, an unknown owner, a name already used by
  that owner, an owner at the limit, and an address Miniflux can't read
  as a feed, each with the reason.
- Exit codes as in feature 001: 0 success, 1 refused or failed.

`sesh doctor` gains `feeds`:
- pass: the number of feeds per owner;
- warn: the gateway can't reach Miniflux, or a feed's last update
  failed (named, with the reason).

Admin endpoints behind them (approvals listener, admin token):

| Endpoint | Body | Reply |
|---|---|---|
| `POST /admin/feeds/add` | `{owner, url, name?}` | `200 {owner, name, url}`; `404` unknown owner; `422 {error}` refused |
| `POST /admin/feeds/remove` | `{owner, name}` | `200`; `404` no such feed |
| `GET /admin/feeds` | | `{state: "ok", feeds: [{owner, name, url, error?}]}`, or `{state: "unavailable", message}` |
