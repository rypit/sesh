# Feature Specification: Memory

**Feature Branch**: `claude/memory`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Proceed with 011 (memory)." Architecture phase 11: the agent's memory provider plugin, backed by the gateway; memory behind the gateway's `memory` connector, scoped per user; recall across stores with documents. Accept: a fact saved in one session is recalled in a later one; user A's agent can't recall user B's facts or documents. Decided with the household (2026-11-28): memories live in the gateway's own database (not Mem0), and embeddings come from a small CPU embedding container in the stack.

## User Scenarios & Testing *(mandatory)*

Actors:
- **A household member** chats with an agent and expects it to remember what they told it.
- **The operator** looks after the stack.
- **An attacker** controls something the agent reads, or the model's output, and tries to plant false memories, read someone else's, or use memory to smuggle instructions.

### User Story 1 - The agent remembers across conversations (Priority: P1) 🎯 MVP

A household member tells the agent "Kari's birthday is 3 May" or "I'm vegetarian". In a later chat, days later, they ask "when is Kari's birthday?" or "suggest a dinner", and the agent answers using what it was told, without being reminded.

**Why this priority**: It's the point of memory, and what the architecture asks phase 11 to prove.

**Independent Test**: In stub mode, the agent saves a fact in one chat. In a new chat with no shared history, its first request to the model already contains the fact, and the answer uses it.

**Acceptance Scenarios**:

1. **Given** a chat, **When** the person tells the agent a fact worth keeping and the agent saves it, **Then** it is stored for the agent's owner.
2. **Given** a saved fact, **When** a later chat asks something related, **Then** the agent's context includes the fact before it answers, without the agent having to ask for it.
3. **Given** a saved fact, **When** the question uses different words ("my sister's birthday" for "Kari is my sister, born 3 May"), **Then** it is still found (search by meaning, not only by words).
4. **Given** the agent saves a fact that restates one it already has, **When** it is saved, **Then** the existing fact is updated rather than duplicated.

---

### User Story 2 - Each person's memories are their own (Priority: P1)

Ana's agent remembers things about Ana; Ben's agent can never recall them. Both can recall what is shared with the Household. A personal agent can add to the Household's memory only with the person's approval.

**Why this priority**: Constitution IV. A memory leak between people is the worst failure of this feature.

**Independent Test**: Facts saved by Ana's agent and by Ben's agent; each agent recalls only its owner's and the Household's; asking with another owner's name or any extra argument is refused. A personal agent's write to the Household waits for approval.

**Acceptance Scenarios**:

1. **Given** facts saved for Ana and for Ben, **When** Ana's agent recalls anything, **Then** it never gets Ben's facts, and vice versa.
2. **Given** Household facts, **When** a personal agent recalls with the Household included, **Then** it gets its owner's facts and the Household's, each labelled.
3. **Given** a personal agent, **When** it saves a fact to the Household, **Then** the person is asked first; the Household agent saves to the Household directly.
4. **Given** Ana's documents and Ben's documents, **When** Ana's agent searches memory and documents together, **Then** it gets only what Ana may read.

---

### User Story 3 - People can see and remove what an agent remembers (Priority: P2)

A household member asks the operator (or the agent) to forget something; the operator can list an owner's memories and remove one or all.

**Why this priority**: Memory must be correctable, and stale facts must be removable; a page for each person comes with 012.

**Independent Test**: `sesh memory list ana` shows Ana's facts (only hers); `sesh memory forget ana <id>` removes one; the agent can forget a fact by its id; the agent can't forget another owner's fact.

**Acceptance Scenarios**:

1. **Given** saved facts, **When** the operator lists an owner's memories, **Then** only that owner's facts appear, newest first.
2. **Given** a fact id, **When** the operator or the owner's agent forgets it, **Then** it is gone from recall.
3. **Given** another owner's fact id, **When** an agent tries to forget it, **Then** it is refused and nothing changes.

---

### User Story 4 - Memory can't be used to steer the agent (Priority: P2)

Text an attacker plants (from an email, a web page, a document) might get saved as a "fact". Recalled facts are always shown to the model as data, never as instructions, and are limited in size and number.

**Why this priority**: Constitution VI: agent-written content never becomes trusted.

**Independent Test**: A saved fact containing "ignore previous instructions…" comes back labelled as untrusted data inside the recall block, and the gateway's own instructions are never altered by it; oversized facts are refused.

**Acceptance Scenarios**:

1. **Given** any recalled fact, **When** it reaches the model, **Then** it is inside a block marked as untrusted data written by agents.
2. **Given** a fact longer than the limit, **When** the agent saves it, **Then** it is refused with the limit.
3. **Given** an owner at the fact limit, **When** the agent saves another, **Then** it is refused until something is forgotten.

### Edge Cases

- **The embedding container is down or slow**: saving still works (the fact is embedded later) and recall falls back to word search; `sesh doctor` reports it.
- **Nothing relevant is remembered**: no recall block is added to the chat at all.
- **A trivial message** ("thanks", "ok"): no recall is looked up.
- **The household's languages**: English, primarily. A fact or question in another language is still found (a multilingual model), and the tests check English, one cross-language case, and a fact found in its own language for Arabic, German, Spanish, Italian, French and Mandarin.
- **A user is removed** (012): their memories go with them; for now, `sesh memory forget <owner> --all`.
- **Backups**: memories are in the gateway's database, so they're backed up with it (012).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The gateway MUST offer a `memory` connector with `remember`, `recall` and `forget`; agents reach memory only through it.
- **FR-002**: Memories MUST be stored by the gateway in its own database, each with its owner, taken from the calling agent's token, never from arguments.
- **FR-003**: `recall` MUST search by meaning and by words together and return the most relevant facts, labelled as untrusted and with their owner (you or Household).
- **FR-004**: A personal agent's recall MUST cover its owner's memories and, when asked, the Household's; never another user's.
- **FR-005**: A personal agent writing to the Household's memory MUST get the person's approval first.
- **FR-006**: `remember` MUST update a near-identical existing fact instead of adding a duplicate.
- **FR-007**: Facts MUST be limited in length (500 characters) and number per owner (2,000).
- **FR-008**: Embeddings MUST be computed by a small multilingual embedding model in a container in the stack, reachable only by the gateway, with no route out.
- **FR-009**: If embeddings are unavailable, `remember` MUST still store the fact and `recall` MUST fall back to word search; missing embeddings MUST be filled in later.
- **FR-010**: The agent runtime MUST recall automatically before answering, through a memory plugin that calls only the gateway, and the runtime's own memory MUST stay off.
- **FR-011**: `sesh memory list <owner>` and `sesh memory forget <owner> <id|--all>` MUST let the operator see and remove an owner's memories; output never includes another owner's.
- **FR-012**: Every remember, recall and forget MUST be audited, without the fact's text.
- **FR-013**: `sesh doctor` MUST check the embedding service and that memories have embeddings.
- **FR-014**: Stack tests MUST cover recall in a later session, isolation between owners (memory and documents), the Household approval, forgetting, and the untrusted labelling.
- **FR-015**: The decisions MUST be recorded in an ADR (memory in the gateway instead of Mem0, the embedding container, the plugin), with the architecture document and diagram updated.

### Key Entities

- **Memory**: a fact, its owner (a user or the Household), the agent that wrote it, when it was written and last updated, and its embedding.
- **Embedding service**: the container that turns text into vectors for search by meaning.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the stub-mode stack test, a fact saved in one chat is in the context of a new chat's first model request, 100% of the time.
- **SC-002**: 0 of another owner's facts are returned across every isolation probe.
- **SC-003**: A paraphrased question finds the fact in the top 3 results with the real embedding model.
- **SC-004**: Recall adds under 300 ms to a chat turn on the household server (excluding the model).
- **SC-005**: 0 fact texts appear in the audit log.
- **SC-006**: Every existing stack test still passes.

## Assumptions

- **Store**: a table in the gateway's database with pgvector (already in the Postgres image); the migration creates the `vector` extension as the superuser.
- **Model**: `intfloat/multilingual-e5-small` (118M parameters, 384 dimensions, MIT, about 100 languages), served by Hugging Face text-embeddings-inference on CPU, baked into a sesh-built image so nothing is downloaded at runtime.
- **No automatic extraction**: agents save facts deliberately with `remember`; extracting facts from chats automatically is later work.
- **The runtime's own memory files** (Hermes `MEMORY.md`, `USER.md`) stay off; skills come with 012.
- **A memory page for each person** comes with 012 (it needs Open WebUI users mapped to sesh users); until then, the operator CLI.
