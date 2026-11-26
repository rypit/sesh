# 0003. One monorepo for code; per-user and household repos kept separate

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

Each agent profile keeps self-learned skills in a git repository with
hourly commits, and household skills live in a separate read-only repo.
Later features may add other per-user git integrations. The question is
where those repositories live relative to this one.

## Decision

- **This repository (`rypit/sesh`) is the project monorepo**: Rust
  crates, the executor's Code Mode configuration and connector stubs, the Hermes plugin, the browser controller,
  deployment templates (Compose), operator skills, specs and decisions.
- It holds **no household data** and **no per-user or per-agent
  repositories**: no skills, memories, documents, chats or
  instance-specific configuration.
- **Per-user and household repositories** (each profile's skills repo,
  the household skills repo, and any later per-user git integration) are
  created by `sesh user add` from templates in this repo, live on the
  server under sesh's data directory, and are local by default. A remote
  for any of them is an operator choice per repository, never GitHub by
  default.
- Promotion of agent-written content into this repo, or from a personal
  skills repo to the household repo, is by human copy or approved pull
  request only (constitution VI).

## Alternatives

- **Everything in the monorepo** (skills as subdirectories): agent-written
  content would sit beside the gateway's source and could reach it through
  an ordinary commit, and household data would follow the code to GitHub.
- **A remote per user by default**: convenient backups, but puts
  household data on a third-party service against local-first.

## Consequences

- The admin CLI owns creating, backing up, archiving and removing these
  repos (`sesh user add/remove`, `sesh backup`).
- `.gitignore` excludes sesh's data directory and any local instance
  configuration; a check in `make check` fails if instance data is
  committed.
