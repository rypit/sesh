# Quickstart: Service isolation

## New installs

Nothing changes: `sesh init && sesh up`. Each service gets its own
Postgres login and its own Redis.

## Upgrading an existing install

```bash
sesh up        # generates the new passwords, hands each database to its
               # service's login, restarts the services, swaps Redis
sesh doctor    # db-isolation: pass
```

What you lose: only transient state in the old shared Redis.
- Open WebUI: open tabs reconnect; nobody is signed out.
- Infisical: queued background jobs and its cache, rebuilt on demand.
- Paperless: tasks queued at that moment (a document being consumed).
  Upgrade when no upload is in flight, and re-upload anything that
  doesn't appear.

If `sesh up` stops with a lock timeout, a service held a database lock
for over a minute; run `sesh up` again.

Once everything works, remove the old Redis volume:

```bash
docker volume rm sesh_redis-data
```

## Checking it yourself

```bash
# Each login opens only its own database:
docker compose -p sesh exec postgres psql -U paperless -d paperless -c 'select 1'   # works
docker compose -p sesh exec postgres psql -U paperless -d gateway -c 'select 1'     # refused
# Who owns what:
docker compose -p sesh exec postgres psql -U sesh -d sesh -c '\l'
```
