# Contract: CLI changes

## `sesh up`

- **Unchanged:** it brings the stack up with `--remove-orphans`.
  Compose recreates any container whose networks changed.
- **New:** afterwards it removes the network `sesh_default` if it
  exists and no container is attached, and prints:

  ```
  removed the old shared network sesh_default (specs/008)
  ```

- **If containers are still attached**, it doesn't fail. It prints a
  warning naming them, and the doctor's `network-layout` check fails
  until they're gone.
- **A second `sesh up`** prints nothing new.

## `sesh up`: database login rules

After `up postgres`, `sesh up`:
1. reads each `*-db` network's subnets;
2. writes the rules (contracts/networks.md);
3. reloads Postgres.

It prints nothing when the rules haven't changed. When they have, it
prints:

```
database login rules updated (each login accepted only from its own network)
```

If a network's subnet can't be read, it fails:

```
error: can't read the address range of sesh_litellm-db; database logins stay refused
```

## `sesh doctor`: `network-layout`

- **Pass:** every running sesh container is attached to exactly the
  networks the layout table names for it, and no `sesh_default`
  exists.
- **Fail:** the detail names each problem, for example:
  - `open-webui is on secrets (expected openwebui-db, models, openwebui-redis, webui-edge)`
  - `postgres is on webui-edge`
  - `sesh_default still exists (used by: ntfy)`

  The fix line says `sesh up`. For a container attached by hand, it
  names `docker network disconnect`.
- **Also checks** that Postgres's active rules (`pg_hba_file_rules`)
  match the current subnets: each service login has exactly its own
  network's ranges, there is no network rule for the superuser, and
  the final rule rejects everything else.
- **Not checked:** stopped containers, and executors' `exec-*`
  networks, which the existing `executors-healthy` check covers.
