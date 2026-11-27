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
- **Not checked:** stopped containers, and executors' `exec-*`
  networks, which the existing `executors-healthy` check covers.
