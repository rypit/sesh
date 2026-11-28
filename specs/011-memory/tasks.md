---
description: "Task list for feature 011: memory"
---

# Tasks: Memory

**Input**: `specs/011-memory/` (spec, plan, research, data-model, contracts)

**Tests**: required (constitution X); each unit test is written failing first, and `memory_stack` before the Compose changes.

## Phase 1: Setup

- [x] T001 ADR 0015 (accepted: the household's decisions of 2026-11-28).
- [x] T002 `deploy/embed.Dockerfile`:
  - a TEI base per `TARGETARCH` from the two pinned digests;
  - the model downloaded at build;
  - runs with the local path and no network at runtime.

  Built in CI's `images` job as `sesh-embed:dev`. After the first CI build, pin the model's commit and file SHA-256s.
- [x] T003 Migration: `CREATE EXTENSION IF NOT EXISTS vector` (superuser) and the `memories` table and indexes in `deploy/postgres/gateway.sql`, granted to the gateway's login only.

## Phase 2: Foundational

- [x] T004 Unit tests, then `crates/sesh-gateway/src/embed.rs`:
  - an OpenAI-style `/v1/embeddings` client with E5 prefixes, a timeout and 384 dimensions;
  - formatting vectors as pgvector literals.
- [x] T005 Unit tests, then the stub's `/v1/embeddings`: deterministic hashed bag-of-words vectors (384, normalised).
- [x] T006 Layout:
  - the `embed` network (`embed`, the gateway) and the listener `embed:80`;
  - Compose (`embed` in `deploy/compose.yaml`, not started in stub mode unless `SESH_REAL_EMBED=1`);
  - the gateway's `[memory]` settings (the stub URL in stub mode).

## Phase 3: US1 + US2 — remember and recall, per owner (P1) 🎯 MVP

- [x] T007 Stack test (written first), `memory_stack` part 1:
  - Ana's agent remembers, and a new MCP session recalls;
  - Ben's agent never sees Ana's facts;
  - Household facts are recalled with `includeHousehold`;
  - a personal Household write waits for approval;
  - a near-duplicate updates;
  - the limits hold;
  - no fact text is in the audit log.
- [x] T008 Unit tests, then `connectors/memory.rs`:
  - the descriptor (`remember` is gated when a personal agent sets `household`, via the dynamic gate from 010);
  - remember (dedup, limits, embed or leave null);
  - recall (fusion, scope, untrusted label);
  - forget (own scope);
  - filling in missing embeddings in the background.
- [x] T009 Cross-store: Ana's agent runs one script with `memory.recall` and `docs.search` and gets nothing of Ben's (part of T007).

## Phase 4: US1 — automatic recall in chats (P1)

- [x] T010 `deploy/hermes/plugins/sesh_memory/__init__.py`:
  - the provider: `prefetch` calls the gateway, a 3 s budget, skips trivial messages, an untrusted block;
  - Hermes config: `memory.provider: sesh_memory`, built-in memory off;
  - the mount, read-only, in `hermes.rs`.

  Unit-tested with the stack's Python inside the runtime image.
- [x] T011 Stack test part 2 (with the runtime and `STUB_SCRIPT=agent`):
  - `[agent:remember]` saves a fact in one chat;
  - in a new chat, the stub's recorded request to the model contains the fact inside the untrusted block (SC-001).

## Phase 5: US3 + US4 — see, forget, and stay data (P2)

- [x] T012 Admin API `/admin/memory` (list, forget) and `sesh memory list|forget`; unit tests per contracts.
- [x] T013 Stack test part 3:
  - the CLI lists only the owner's facts;
  - forget removes one;
  - an agent can't forget another owner's;
  - a fact with "ignore previous instructions" comes back inside the untrusted block.
- [x] T014 Stack test part 4 (`SESH_REAL_EMBED=1`): the real model finds a paraphrase in the top 3, in Norwegian and English (SC-003); with the embedding service stopped, remember still works and recall falls back to words; after a restart, missing embeddings are filled in.

## Phase 6: Polish

- [x] T015 `sesh doctor`'s `memory` check (the embedding service answers; no fact is missing its embedding for long).
- [x] T016 Architecture doc (Mem0 replaced, the embed service, phase 11), the diagram (Mem0 → Memory, embed), README, AGENTS.md.
- [x] T017 CI: build `sesh-embed:dev`; add `memory_stack` (the matrix guard enforces it).
- [ ] T018 Record CI results and the model pin in research.md.
