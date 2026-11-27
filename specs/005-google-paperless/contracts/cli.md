# Contract: CLI additions

```
sesh user add <name>            # a user and their approval topic; prints the ntfy username and where the password is
sesh user remove <name>         # disconnects Google; their agents stop working
sesh user list                  # name, topic, Google state
sesh agent add <name> --connectors … [--owner <user>|household]   # owner defaults to household
sesh google connect <user|household>      # prints a one-time consent link
sesh google disconnect <user|household>
```

`sesh doctor` gains two checks:
- `google`: each user's connection state (warns for revoked or
  missing);
- `paperless`: the gateway can reach Paperless.
