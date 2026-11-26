# Research: Foundation stack

Sources read 2026-11-26: Open WebUI's `backend/open_webui/config.py`,
`env.py` and the docs repo's `docs/reference/env-configuration.mdx`
(GitHub raw); release pages for Open WebUI (v0.13.5) and LiteLLM
(v1.111.0); the pgvector README. The Open WebUI and LiteLLM doc sites
were not reachable from the build environment, so LiteLLM's behaviour
was tested directly (R2).

## R1. Open WebUI settings

**Decision**: configure Open WebUI only through environment variables,
with `ENABLE_PERSISTENT_CONFIG=False` so the environment always wins over
values stored in its database.

| Requirement | Setting (default) |
|---|---|
| Built-in memory off (FR-005) | `ENABLE_MEMORIES=False` (True) |
| Admin can't open others' chats | `ENABLE_ADMIN_CHAT_ACCESS=False` (True) |
| No open sign-up | `ENABLE_SIGNUP=False` (True); first admin from `WEBUI_ADMIN_EMAIL` + `WEBUI_ADMIN_PASSWORD`, created on first start when no users exist |
| Task model | `TASK_MODEL_EXTERNAL=agent-main` (empty) |
| Only the alias router as a source | `OPENAI_API_BASE_URL=http://litellm:4000/v1`, `OPENAI_API_KEY=<client key>`, `ENABLE_OLLAMA_API=False` (True) |
| Postgres and Redis | `DATABASE_URL=postgresql://…`, `REDIS_URL=redis://…`, `WEBSOCKET_MANAGER=redis` |
| Session signing | `WEBUI_SECRET_KEY` (required when auth is on; startup exits without it) |

**Raw models hidden**: Open WebUI can only list what its single source
lists, and the alias router lists only aliases (R2), so raw names never
reach the model picker. Whether connection models are visible to
non-admins by default isn't stated in the sources read;
`BYPASS_MODEL_ACCESS_CONTROL` defaults to False. **Decision**: set
`BYPASS_MODEL_ACCESS_CONTROL=True` in this phase so non-admins see the
router's aliases, which are exactly the sanctioned set. When per-user
agents arrive (each user sees only their own), model access control is
switched back on in that feature. The stack test checks that a non-admin
sees exactly `agent-main`.

**Local-first**: set `OFFLINE_MODE=True`, `DO_NOT_TRACK=True`,
`SCARF_NO_ANALYTICS=True`, `ANONYMIZED_TELEMETRY=False` and turn off RAG
embedding auto-download, so Open WebUI makes no outbound calls of its
own. **Verify** in the stack test that it starts and serves chat with
no internet egress except to the alias router.

**Health**: `/health` (to be confirmed by the stack test; not stated in
the sources read).

**Image**: `ghcr.io/open-webui/open-webui:v0.13.5`.

## R2. LiteLLM as the alias router

Tested with `litellm[proxy]==1.111.0` from PyPI, a config mapping
`agent-main` to `openai/raw-a` on a fake upstream that lists `raw-a` and
`raw-b`, and a master key.

| Behaviour | Result |
|---|---|
| `/v1/models` | lists only `agent-main` |
| request for `agent-main` | routed upstream as `raw-a` |
| request for `raw-a`, `raw-b`, `openai/raw-b`, `openai/gpt-4o`, `anthropic/claude-x` | refused: "Invalid model name passed in model=…" (400) |
| request for `openai/raw-a` (the deployment's own model string) | **accepted**, routed to the same upstream model |
| no key | 401 |
| health | `/health/liveliness` → "I'm alive!", `/health/readiness` → healthy |

**Decision**:

- `model_list` maps each alias to `openai/<upstream model>` with the
  operator's `api_base`; no wildcard entries.
- To close the `openai/<model>` gap, clients never use the master key.
  `sesh up` creates a **virtual key per client** (Open WebUI now; agents
  later) restricted to the alias list. Virtual keys need LiteLLM's
  database: a `litellm` database in the shared Postgres. **Verify** in the
  stack test that a restricted key is refused for `openai/<model>`; if
  LiteLLM still accepts it, record the gap in the risks table, since it
  only ever reaches the model already behind the alias.
- Image: `ghcr.io/berriai/litellm:v1.111.0`.
- Telemetry off: `LITELLM_TELEMETRY=False` (verify the variable name in
  the stack test by checking for no egress).

**Alternatives**: Open WebUI's own per-model presets and access control
(declarative setup is awkward, state lives in its database); a custom
alias proxy (against adopt-before-build).

## R3. Postgres and Redis

- `pgvector/pgvector:0.8.7-pg16` (README lists `0.8.7-pg16`), with
  databases `openwebui` and `litellm` created by an init script; later
  phases add theirs.
- Redis 7 (`redis:7.4-alpine`) with a password, for Open WebUI's
  websocket manager now and later components afterwards.
- Neither publishes a port: only containers on the stack network reach
  them.

## R4. Exposure

- Only Open WebUI (`127.0.0.1:3000`) and the alias router
  (`127.0.0.1:4000`, for `sesh doctor` and future host-side clients)
  publish ports, both on loopback.
- People reach Open WebUI through `tailscale serve --bg 3000` (tailnet
  HTTPS); this phase documents it and `sesh doctor` checks for it
  (warning only), using `tailscale serve status --json`.

## R5. Credentials before the secret store

`sesh init` generates the Postgres, Redis, LiteLLM master and Open WebUI
secret values with the OS random source, writes them to
`$SESH_DATA_DIR/secrets.env` (mode 0600), and never prints them. The
data directory defaults to `~/.local/share/sesh` and is outside the
repository. Phase 4 moves these into the secret store.

## R6. Stub model server

A small Rust binary (`crates/sesh-stub-llm`, built into a container image
from this repo) serving `/v1/models` (two raw models), and
`/v1/chat/completions` with streaming and non-streaming replies. When a
request carries tools it returns a call to the first tool, so tool-calling
checks are deterministic. Built on `tiny_http` + `serde_json`, no async
runtime.

## R7. `sesh doctor` and `sesh up`

- `sesh init`: write `sesh.toml` (inference URL, alias → model) and
  `secrets.env` if missing; render `litellm.yaml`. Idempotent.
- `sesh up`: `docker compose up` the stack, create the Open WebUI client
  key on first run, then start Open WebUI.
- `sesh doctor`: checks in contracts/cli.md. Talks to Docker only through
  `docker compose` (one seam), to HTTP through `ureq`, to Tailscale
  through its CLI (one seam each).

## R8. Images and the build environment

Docker Hub rate-limits anonymous pulls and the GitHub registry's blob host
is not reachable from the cloud build environment, so stack tests run in
GitHub Actions. Images are pinned by tag in this phase; pinning by digest
is a task once CI can resolve them.
