# Implementation Plan: Memory

**Branch**: `claude/memory` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

## Summary

- **`memory` connector** in the gateway: `remember`, `recall` and `forget`, with the owner taken from the token. Memories live in a `memories` table in the gateway's database (pgvector and full-text), searched by reciprocal rank fusion (research.md R1–R3).
- **Embeddings** come from a text-embeddings-inference CPU container running `multilingual-e5-small`, baked into a sesh-built image. It sits on an `embed` pair network with the gateway. Stub mode uses the stub's deterministic embeddings (R4).
- **Automatic recall** comes from a Hermes memory provider plugin. It is mounted read-only into each agent runtime and calls only the gateway (R5).
- **Household writes** from personal agents are gated (R6).
- **Operator CLI**: `sesh memory list|forget`. Doctor gains `memory`.
- **ADR 0015**, the architecture text (Mem0 replaced) and the diagram.

## Technical Context

**Language/Version**:
- Rust 1.97 (`#![forbid(unsafe_code)]`);
- the plugin in Python 3, inside the untrusted runtime (ADR 0002);
- SQL;
- a Dockerfile for the embedding image.

**Primary Dependencies**: `pgvector` (already in the Postgres image); TEI 1.10.0 (new adopted component); no new Rust crates (vectors are sent as text literals, `'[0.1,…]'::vector`).

**Storage**: a `memories` table and its indexes in the gateway database; the migration creates the `vector` extension as the superuser.

**Testing**:
- unit tests: the fusion ranking, the dedup rule, scoping, limits, the gate, the overlay and layout, the plugin's request building (Python, run in the stack test);
- stack test: `memory_stack` (new, with the real embedding container), plus the layout checks in `network_stack`.

**Constraints**: CI has no container internet; the model is baked in at image build.

## Constitution Check

| Principle | Check |
|---|---|
| I. Only the control plane is trusted | The embedding container is untrusted: one internal network to the gateway, no route out. The plugin runs inside the untrusted runtime and holds only the agent's token. |
| II. One way out | The agent reaches memory only through the gateway; the plugin calls only the gateway. |
| III. Agents never see secrets | No new secrets reach agents. |
| IV. Per-user isolation | The owner comes from the token; recall covers the owner and, optionally, the Household; tests prove isolation for memory and documents. |
| V. Side effects need approval | A personal agent's Household writes are gated. |
| VI. Agent content is data | Facts are agent-written: they are always recalled inside an untrusted-data block, never as instructions or configuration. |
| VII. Local-first | Embeddings are computed on the household server; nothing leaves the house. |
| VIII. Adopt before build | TEI and pgvector are adopted; Mem0 was considered and rejected for this scope (ADR 0015). |
| X. Every guarantee is executable | Each requirement has a unit or stack test. |

## Project Structure

```text
crates/sesh-gateway/src/connectors/memory.rs   connector, store, fusion, dedup
crates/sesh-gateway/src/embed.rs               the embedding client (TEI or the stub)
deploy/postgres/gateway.sql                    vector extension, memories table
deploy/embed.Dockerfile                        TEI + baked model
deploy/hermes/plugins/sesh_memory/__init__.py  the recall plugin
crates/sesh-cli/src/{up,hermes,layout,doctor,memory,main}.rs
crates/sesh-stub-llm/src/main.rs               /v1/embeddings (stub)
crates/sesh-cli/tests/stack.rs                 memory_stack
docs/decisions/0015-memory.md
```
