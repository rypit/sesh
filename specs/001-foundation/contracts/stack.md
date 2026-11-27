# Contract: the Compose stack (`deploy/compose.yaml`)

| Service | Image | Published | Internal | Health |
|---|---|---|---|---|
| `postgres` | `pgvector/pgvector:0.8.7-pg16` | none | 5432 | `pg_isready` |
| `redis` | `redis:7.4-alpine` | none | 6379 | `redis-cli ping` |
| `litellm` | `ghcr.io/berriai/litellm:v1.111.0` | `127.0.0.1:${router}:4000` | 4000 | `/health/liveliness` |
| `open-webui` | `ghcr.io/open-webui/open-webui:v0.13.5` | `127.0.0.1:${webui}:8080` | 8080 | `/health` |
| `stub-llm` (stub overlay only) | built from `deploy/stub.Dockerfile` | none | 8000 | `/v1/models` |

Open WebUI environment (research.md R1): `ENABLE_PERSISTENT_CONFIG=False`,
`ENABLE_MEMORIES=False`, `ENABLE_ADMIN_CHAT_ACCESS=False`,
`ENABLE_SIGNUP=False`, `WEBUI_ADMIN_EMAIL`, `WEBUI_ADMIN_PASSWORD`,
`TASK_MODEL_EXTERNAL=agent-main`, `ENABLE_OLLAMA_API=False`,
`OPENAI_API_BASE_URL=http://litellm:4000/v1`,
`OPENAI_API_KEY=${WEBUI_ROUTER_KEY}`, `BYPASS_MODEL_ACCESS_CONTROL=True`,
`DATABASE_URL`, `REDIS_URL`, `WEBSOCKET_MANAGER=redis`,
`WEBUI_SECRET_KEY`, `OFFLINE_MODE=True`, `DO_NOT_TRACK=True`,
`SCARF_NO_ANALYTICS=True`, `ANONYMIZED_TELEMETRY=False`.

Volumes: `postgres-data`, `redis-data`, `open-webui-data`. Network: one
internal bridge network; only the two published ports reach the host.

Image digests (multi-architecture index, resolved 2026-11-26):

| Image | Digest |
|---|---|
| `pgvector/pgvector:0.8.7-pg16` | `sha256:484e838076e04939d27d3e176e91c86eea96236126023be8d3059761122e579f` |
| `redis:7.4-alpine` | `sha256:7f7d30b7fdd09467bac58198a1f166dd69789d348d4fe859cccb20e0cf287fbe` |
| `ghcr.io/berriai/litellm:v1.111.0` | `sha256:ce00096d012719d58c52cecfa373f7c6256dd1b0ff08f6fa607e10e9628f1f46` |
| `ghcr.io/open-webui/open-webui:v0.13.5` | `sha256:e0dc5a8bf5c71cadf339d1e7b9408b357da75fb2854f82cad57a1609fadf2a22` |
