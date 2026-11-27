# Implementation Plan: Foundation stack

**Branch**: `001-foundation` | **Date**: 2026-11-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-foundation/spec.md`

## Summary

One Compose file brings up Postgres (pgvector), Redis, LiteLLM as the
alias router, and Open WebUI. Clients only ever see model aliases
(`agent-main`), served from whatever OpenAI-compatible server the
operator configures. Only Open WebUI and the router publish ports, on
loopback; people reach Open WebUI through Tailscale. A Rust workspace
starts with `sesh init`, `sesh up` and `sesh doctor`, plus a stub model
server for development and CI. `make check` and GitHub Actions run the
checks and the stack tests.

## Technical Context

**Language/Version**: Rust, stable, pinned in `rust-toolchain.toml`
(ADR 0002); YAML for Compose and LiteLLM config.

**Primary Dependencies**: `clap`, `serde`, `toml`, `serde_json`,
`anyhow`, `ureq` (HTTP client), `getrandom` (secret generation),
`tiny_http` (stub server only). Adopted images: `pgvector/pgvector`,
`redis`, `ghcr.io/berriai/litellm`, `ghcr.io/open-webui/open-webui`.

**Storage**: Postgres (databases `openwebui`, `litellm`) and Redis in
named Docker volumes; sesh's own files (`sesh.toml`, `secrets.env`,
`litellm.yaml`) in `$SESH_DATA_DIR` (default `~/.local/share/sesh`),
outside the repo.

**Testing**: `cargo test` (unit; Docker, HTTP and Tailscale faked at their
modules); stack tests in `crates/sesh-cli/tests/stack.rs`, ignored by
default and run by `make stack-test`, which starts the stack with the
stub.

**Target Platform**: the reference Mac (Docker Desktop) and Linux (CI).

**Project Type**: Cargo workspace plus deployment files.

**Performance Goals**: first token within 5 s on a warm server (SC-001);
`sesh doctor` under 30 s (SC-005); CI under 15 min (SC-006).

**Constraints**: loopback-only ports; no secrets in the repo or logs;
no outbound telemetry from adopted components; engine-neutral inference.

**Scale/Scope**: one household, a handful of users.

## Constitution Check

*GATE: checked before Phase 0 and after Phase 1 design. Both pass.*

| Principle / constraint | How this plan complies |
|---|---|
| I. Control plane is the only trusted zone | No agents yet; Open WebUI and LiteLLM hold no household credentials beyond their own local keys. |
| II. One way out | Clients reach models only through the router; Postgres and Redis publish no ports. |
| III. Agents never see secrets | Generated secrets live in `secrets.env` (0600) outside the repo, never printed; Open WebUI gets a restricted client key, never the router's master key. |
| IV. Per-user isolation | Admin chat access off; sign-up off. |
| V. Side effects need approval | No side-effecting tools in this phase. |
| VI. Agent-authored content never trusted | Built-in memory off; no agent content yet. |
| VII. Local-first | Inference server on the home network; telemetry and model auto-download off in adopted components (R1, R2). |
| VIII. Adopt before build | Adopts Postgres, Redis, LiteLLM, Open WebUI, Tailscale; builds only the CLI and a test stub. |
| IX. Swappable inference | One inference setting group; aliases only (ADR 0005). |
| X. Every guarantee is executable | Negative stack tests for loopback, raw model refusal, non-admin model list, admin chat access (FR-012). |
| Languages | CLI and stub in Rust, `unsafe` forbidden. |
| Repository boundaries | Data and generated config in `$SESH_DATA_DIR`; `.gitignore` covers it. |

## Project Structure

### Documentation (this feature)

```text
specs/001-foundation/
├── spec.md
├── plan.md            # this file
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli.md         # sesh init / up / doctor
│   ├── config.md      # sesh.toml, secrets.env, rendered litellm.yaml
│   └── stack.md       # services, ports, settings
└── tasks.md           # /speckit-tasks
```

### Source Code (repository root)

```text
Cargo.toml                    # workspace
rust-toolchain.toml
deny.toml
Makefile                      # check, stack-test, up, down
crates/
├── sesh-cli/                 # binary `sesh`
│   ├── src/
│   │   ├── main.rs           # clap; exit codes
│   │   ├── config.rs         # sesh.toml model (deny_unknown_fields)
│   │   ├── secrets.rs        # generate / load secrets.env (0600)
│   │   ├── render.rs         # litellm.yaml from config (pure)
│   │   ├── init.rs
│   │   ├── up.rs             # compose up, client key bootstrap
│   │   ├── doctor.rs         # checks and report
│   │   └── host/
│   │       ├── compose.rs    # seam: the only module that runs `docker compose`
│   │       ├── tailscale.rs  # seam: `tailscale serve status --json`
│   │       └── http.rs       # seam: HTTP to the router and Open WebUI
│   └── tests/
│       └── stack.rs          # #[ignore]: needs Docker; run by `make stack-test`
└── sesh-stub-llm/            # binary: OpenAI-compatible stub for dev and CI
    └── src/main.rs
deploy/
├── compose.yaml              # postgres, redis, litellm, open-webui
├── compose.stub.yaml         # adds the stub server (dev and CI)
├── postgres/init.sql         # creates openwebui and litellm databases
└── stub.Dockerfile           # builds sesh-stub-llm
.github/workflows/ci.yml      # make check; stack tests
```

**Structure decision**: one workspace with the CLI and the stub. Each
external tool (Docker Compose, Tailscale, HTTP) has one module. Rendering
the router config is pure, so it is unit-tested without Docker.

## Key flows

**First run**: `sesh init --inference-url http://host:port/v1 --model
<name>` → writes `sesh.toml`, `secrets.env`, `litellm.yaml` → `sesh up` →
Postgres, Redis, LiteLLM start → on first run, sesh creates an Open WebUI
virtual key restricted to the aliases (via LiteLLM's key API with the
master key) and stores it in `secrets.env` → Open WebUI starts.

**Change inference server**: edit `[inference]` in `sesh.toml` (or rerun
`sesh init` with new values) → `sesh up` re-renders `litellm.yaml` and
restarts LiteLLM only.

**Doctor**: see contracts/cli.md.

## Risks

| Risk | Mitigation |
|---|---|
| A restricted key still accepts `openai/<model>` | Stack test detects it; reaches only the alias's own model; recorded if so. |
| Open WebUI makes outbound calls (embeddings, telemetry) | Offline and telemetry settings; stack test without egress. |
| Non-admins can't see `agent-main` with access control on | `BYPASS_MODEL_ACCESS_CONTROL=True` this phase; revisited with per-user agents. |
| Image pulls rate-limited | Stack tests run in GitHub Actions; digests pinned. |
| Open WebUI ConfigVars stick in its database | `ENABLE_PERSISTENT_CONFIG=False`. |

## Complexity Tracking

None.
