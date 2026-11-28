# Data model: Memory

## `memories` (gateway database)

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | primary key, `gen_random_uuid()` |
| `owner` | `text` | a user's name or `household`, from the token |
| `agent` | `text` | the agent that wrote it (audit) |
| `fact` | `text` | at most 500 characters |
| `embedding` | `vector(384)` | null until computed (FR-009) |
| `tsv` | `tsvector` | generated: `to_tsvector('simple', fact)` |
| `created_at`, `updated_at` | `timestamptz` | |

Indexes:
- `(owner, updated_at desc)`;
- GIN on `tsv`;
- HNSW on `embedding vector_cosine_ops`.

Grants: the gateway's login only. No other login can read it (feature 007).

## Gateway settings (`gateway.toml`, new `[memory]`)

| Field | Default | Notes |
|---|---|---|
| `embed_url` | `http://embed:80` | the stub's address in stub mode |
| `max_facts` | 2000 | per owner |
| `max_chars` | 500 | |
| `dedup_similarity` | 0.92 | |

## Network (layout.rs)

`embed` (internal): `embed`, the gateway. The embedding service listens on `:80`.
