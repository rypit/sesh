# 0015. Memory in the gateway's database, embeddings from a local CPU service

- Status: accepted
- Date: 2026-11-28
- Feature: specs/011-memory

## Context

The architecture planned Mem0 (with `infer=False`) behind the gateway's `memory` connector, with CPU embeddings. Research for feature 011 found:
- Mem0 publishes no maintained image (one `latest` tag from September 2025), so we'd build and maintain our own Python image.
- Its telemetry is on by default.
- It keeps its own auth database.
- With `infer=False` it adds little beyond a vector index.

## Decision

The household decided on 2026-11-28:

1. **Memories are a table in the gateway's database** (pgvector and Postgres full-text), owned by the gateway's login.
   - The owner always comes from the token.
   - Search fuses search by meaning and by words.
   - A near-duplicate fact is updated, not added.
   - Mem0 is dropped.
2. **Embeddings come from a small CPU container**: Hugging Face text-embeddings-inference with `multilingual-e5-small`, baked into a sesh-built image.
   - It sits on one internal network shared only with the gateway, with no route out.
   - In stub mode the gateway uses the stub's deterministic embeddings.
3. **Automatic recall comes from a Hermes memory provider plugin** (Python, inside the untrusted runtime, mounted read-only). It calls only the gateway, with the agent's own token, and always presents facts as untrusted data. Saving and forgetting go through code mode like every other call.
4. **A personal agent's writes to the Household's memory are gated.**

## Alternatives

- **Mem0 as planned**: rejected for now (above). If automatic fact extraction is wanted later, it can be built on the same table with `agent-main`.
- **Embeddings from the inference server** (a LiteLLM alias): simpler, but ties memory to the GPU host and its model choices. The household preferred a self-contained service.
- **Word search only**: no model needed, but it misses paraphrases ("my sister's birthday").

## Consequences

- One more adopted component (TEI) and one more sesh-built image. CI builds it, and only `memory_stack` runs it.
- The `vector` extension is created by the superuser in the migration (the architecture's recorded constraint).
- Memory is backed up with the gateway's database.
- Changing the embedding model means re-embedding every fact. The gateway does it in the background, like filling in facts saved while the embedding service was down.
