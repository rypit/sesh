# Research: Memory

Sources:
- Mem0's repository (mem0ai/mem0, `server/`, `pyproject.toml`, mem0ai 2.2.1);
- Docker Hub's `mem0/mem0-api-server` tags;
- Hermes Agent v0.23.4's source in its image (`agent/memory_provider.py`, `plugins/memory/__init__.py`);
- the GitHub container registry's tags for `huggingface/text-embeddings-inference`;
- the Hugging Face Hub entry for `intfloat/multilingual-e5-small`.

All read on 2026-11-28.

## R1. Where memories live: the gateway's database, not Mem0

**Finding (Mem0)**:
- The OSS server (`server/main.py`) is a FastAPI app over the `mem0ai` package.
- It needs pgvector plus an OpenAI-compatible embedder and LLM.
- It keeps its own auth database (SQLAlchemy and Alembic) and a SQLite history.
- Its anonymous telemetry (PostHog) is on by default.
- Docker Hub has only `mem0/mem0-api-server:latest`, last updated September 2025, so there is no maintained release to pin. We would build and maintain a Python image, larger with the `[nlp]` extras the architecture wanted for keyword search.
- With `infer=False` (the architecture's plan), Mem0 adds little beyond a vector index.

**Decision** (the household, 2026-11-28): memories are a table in the gateway's own database.
- The owner always comes from the token, like every other connector.
- Search is hybrid: pgvector cosine similarity plus Postgres full-text (`simple` configuration, so Norwegian and English words both match).
- No new service, network or database login.

**Alternatives**: Mem0 as planned (above); a separate vector database (another adopted service, no gain at household scale).

## R2. Search: reciprocal rank fusion of meaning and words

**Decision**: `recall(query)` runs two queries over the owner's readable memories:
- the 20 nearest by cosine distance;
- the 20 best by `ts_rank` on a generated `tsvector`.

It fuses them with reciprocal rank fusion (`score = Σ 1/(60 + rank)`) and returns the top `limit` (default 5, at most 20). Without an embedding for the query (the service is down), only the word query runs. Indexes: HNSW on the embedding, GIN on the `tsvector`, and a btree on the owner.

## R3. Updating instead of duplicating

**Decision**: `remember(fact)` first looks for an existing fact of the same owner with cosine similarity of at least 0.92, or identical normalised text. If one exists, it replaces that fact's text (keeping its id) and reports `updated`; otherwise it inserts. This is what the architecture's "Mem0 is add-only" risk asked for.

## R4. Embeddings: text-embeddings-inference on CPU

**Finding**:
- Hugging Face text-embeddings-inference (TEI) publishes CPU images per architecture: `cpu-1.10.0` (amd64, index digest `sha256:f0a4bb206b9d4877746b5b9f638e138fe683eeb9787c14d541a12c993b562d86`) and `cpu-arm64-1.10.0` (arm64, `sha256:446c683eef5bbbdf1eb9899966b8e198921f671e92ee3fe3d7a5f6d770ea0a26`).
- It serves an OpenAI-compatible `/v1/embeddings` and `/embed`.
- `intfloat/multilingual-e5-small`: BERT, 118M parameters, 384 dimensions, MIT, about 100 languages including Norwegian (`no`). `model.safetensors` is 470,641,600 bytes.

**Decision**:
- **Image**: `deploy/embed.Dockerfile` picks the base by `TARGETARCH` from the two pinned digests. It downloads the model files at build time, so the running container never reaches the internet, and the service runs with the local model path.
- **Prefixes**: E5 models expect `query: ` and `passage: ` prefixes; the gateway adds them.
- **Pinning the model**: huggingface.co isn't reachable from the development sandbox, so the first CI build records the model's commit and file SHA-256s. The Dockerfile then pins them (a task).
- **Network**: the container is on one internal pair network, `embed`, with the gateway. It has no edge network and no published port.
- **Stub mode**:
  - the gateway uses the stub's `/v1/embeddings`: deterministic hashed bag-of-words vectors, so word overlap gives similarity;
  - `sesh up` doesn't start the embedding container, so the other stack tests don't carry the image;
  - `memory_stack` runs the real container (`SESH_REAL_EMBED=1`) to check search by meaning (SC-003).

## R5. Automatic recall: a Hermes memory provider plugin

**Finding** (Hermes v0.23.4, `plugins/memory/__init__.py`):
- **Loading**: memory providers are found in `$HERMES_HOME/plugins/<name>/`, with a `MemoryProvider` subclass or a `register_memory_provider` call in `__init__.py`, and activated by `memory.provider`.
- **The interface** (`agent/memory_provider.py`):
  - `name`, `is_available`, `initialize` and `get_tool_schemas` are abstract;
  - `prefetch(query) -> str` returns text added to the turn's context;
  - `sync_turn` and `on_memory_write` mirror writes.

**Decision**:
- **The plugin**: `deploy/hermes/plugins/sesh_memory/__init__.py` (Python, inside the untrusted runtime: ADR 0002), mounted read-only at `/opt/data/plugins/sesh_memory`.
- **`prefetch`**:
  - skips trivial messages;
  - calls the gateway's MCP `execute` with `memory.recall({query, limit: 5, includeHousehold: true})`, using the agent's own token from its environment, with a 3-second budget;
  - returns the facts in a block headed as untrusted, agent-written data.
- **The rest**: it offers no tools (`get_tool_schemas` returns `[]`). Saving and forgetting go through `memory.remember` and `memory.forget` in code mode, so they are gated and audited like every other call.
- **Hermes's own memory**: `MEMORY.md`, `USER.md` and the `memory` toolset stay off.
- **To verify in the stack test**: Hermes runs the provider with its built-in memory off. If it doesn't, the configuration turns on the provider alone and the `memory` toolset stays disabled.

## R6. Writing to the Household

**Decision**:
- `remember({fact, household: true})` from a personal agent is gated (ADR 0007): the person sees the fact's text, capped, in the approval.
- The Household agent's own memories are the Household's, so it needs no approval.
- `forget` works only on the caller's own scope. A personal agent can't forget a Household fact; the operator can.

## R7. Recall across stores

**Decision**: nothing is built. The connector's instructions show one `execute` script calling `memory.recall` and `docs.search` together. Scope comes from the token in both, so the stack test checks that Ana's agent gets neither Ben's facts nor his documents (spec US2).
