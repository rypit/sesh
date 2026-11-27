# Contract: CLI additions

```
sesh user add <name>            # a user and their approval topic; prints the ntfy username and where the password is
sesh user remove <name>         # disconnects Google; their agents stop working
sesh user list                  # name and topic (Google state is in `sesh doctor`)
sesh agent add <name> --connectors … [--owner <user>|household]   # owner defaults to household
sesh google connect <user|household>      # prints a one-time consent link
sesh google disconnect <user|household>
```

`sesh doctor` gains two checks:
- `google`: who is connected; warns when a grant was revoked (missing
  is fine: not everyone uses Google);
- `paperless`: the gateway can sign in to Paperless; warns otherwise
  (it is slow to start, and its absence isn't a safety problem).

Admin endpoints behind them (approvals listener, admin token):
`POST /admin/google/consent {owner}`, `POST /admin/google/disconnect
{owner}`, `GET /admin/google/status`, `GET /admin/paperless`. The person's
pages: `GET|POST /google/start/{id}` (the secret is POSTed from the
fragment) and `GET /google/callback`.
