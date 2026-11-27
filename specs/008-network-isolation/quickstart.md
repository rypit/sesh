# Quickstart: Network isolation

## Fresh install (stub mode)

```bash
sesh init --stub && sesh up
sesh doctor            # network-layout passes
docker network ls --filter label=com.docker.compose.project=sesh
                       # no sesh_default; the networks in contracts/networks.md
```

## Upgrade an existing install

```bash
sesh up                # recreates containers on their new networks,
                       # then: removed the old shared network sesh_default
sesh doctor            # network-layout passes; all other checks as before
sesh up                # nothing new
```

No data changes: volumes are untouched.

## Check the database login rules

```bash
docker compose -p sesh exec -T postgres psql -U sesh -Atc \
  "SELECT type, database, user_name, address, netmask, auth_method FROM pg_hba_file_rules"
# one host line per login, on its own network's range; local trust; host all all all reject
```

## Check reachability by hand

From inside a service's network namespace, using the gateway image,
which has bash:

```bash
id=$(docker compose -p sesh ps -q open-webui)
docker run --rm --network container:$id --entrypoint bash sesh-gateway:dev \
  -c 'timeout 3 bash -c "exec 3<>/dev/tcp/litellm/4000" && echo allowed'
docker run --rm --network container:$id --entrypoint bash sesh-gateway:dev \
  -c 'timeout 3 bash -c "exec 3<>/dev/tcp/infisical/8080" || echo refused'
```

## Tests

```bash
make check                                  # includes the layout unit test
make stack-test STACK_TESTS=network_stack   # the reachability matrix
make stack-test                             # every stack test
```
