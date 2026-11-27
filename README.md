# sesh

[![ci](https://github.com/rypit/sesh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rypit/sesh/actions/workflows/ci.yml?query=branch%3Amain)

A local-first AI agent platform for one household. Each person gets an
always-on agent, plus a shared Household agent, that can handle email,
calendar, documents, browsing and errands. Every agent reaches the world
through **one trusted gateway**, so no agent ever holds a secret, and
anything with side effects waits for a person's yes.

- **Assistant mode**: Hermes agents, reached through Open WebUI or
  Telegram, using Google, memory (Mem0), documents (Paperless-ngx),
  search (SearXNG, Kiwix) and a sandboxed browser, all through the
  gateway.
- **Operator mode**: the `sesh` admin CLI and Claude Code operator
  skills set up users, rotate secrets and run backups. No agent can reach
  them.
- **Local-first**: models run on the home server, on any
  OpenAI-compatible inference server (oMLX, Ollama, llama.cpp, vLLM…);
  household data stays there.

## Architecture

<p align="center">
  <img src="docs/architecture.svg" alt="sesh architecture: household users reach Hermes agents through Open WebUI or Telegram; agents, and the per-agent code executor that runs their scripts, reach Google, search, weather and maps, Paperless-ngx, a sandboxed browser and Mem0 only through the Code Mode gateway, which uses Infisical for secrets, an append-only audit log, and ntfy for approvals; inference runs on any OpenAI-compatible server" width="760">
</p>

Agents are untrusted; the gateway is their only way out. Approvals and
one-time links go from the gateway to people through ntfy, never through
an agent. The design is in the
[Solutions Architecture](docs/architecture/solutions-architecture.md);
see [`docs/architecture/`](docs/architecture/) to update the diagram.

## Guarantees

The [constitution](.specify/memory/constitution.md) is binding. In short:

- Agents are untrusted; the control plane (gateway, secret store,
  approvals, audit log) is the only trusted zone.
- One way out: agents have no tools except the gateway.
- Agents never see secrets; logins are filled by the gateway.
- Each user's agent reaches only that user's data.
- Side effects need a human yes, written by the gateway, not the agent.
- Every guarantee has an automated check.

## Status

**Building.** Features 001 to 004 (foundation stack, gateway core, model
evals, secrets and approvals) are done; feature 005 (Google and
Paperless, with per-user identity,
[`specs/005-google-paperless/`](specs/005-google-paperless/)) and
feature 006 (RSS feeds, [`specs/006-rss-feeds/`](specs/006-rss-feeds/))
are in progress. The design is in the
[Solutions Architecture](docs/architecture/solutions-architecture.md);
decisions are in [`docs/decisions/`](docs/decisions/)
and reference notes in [`docs/research/`](docs/research/). The build
follows the architecture's eleven phases (foundation, gateway core,
model evals, secrets and approvals, Google and Paperless, RSS feeds,
read-only Google Docs, memory, one agent, browser, household), each a
feature under `specs/`.

## Try it

With Docker and Rust installed, no model needed:

```bash
docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
docker build -f deploy/gateway.Dockerfile -t sesh-gateway:dev .
docker build -f deploy/executor.Dockerfile -t sesh-executor:dev .
cargo install --path crates/sesh-cli
sesh init --stub --home 59.91,10.75
sesh agent add test --connectors search,geo
sesh up
sesh doctor
sesh eval      # can the model behind agent-main use sesh's resources?
```

Open WebUI is then on http://127.0.0.1:3000 (admin password in
`~/.local/share/sesh/secrets.env`). For a real model, run
`sesh init --inference-url http://<server>/v1 --model <name>` with any
OpenAI-compatible server, then `sesh up`. The gateway's MCP endpoint is
http://127.0.0.1:7000/mcp; the test agent's token is in
`~/.local/share/sesh/agents/test.token` (never printed). See the
quickstarts for [001](specs/001-foundation/quickstart.md),
[002](specs/002-gateway-core/quickstart.md) and
[003](specs/003-model-evals/quickstart.md).

**Approvals and credentials:**
- **Approvals:** a side effect waits for a yes. The gateway sends an ntfy
  notification to your private topic, `ntfy_topic` in `sesh.toml`.
  Tapping it opens a page that shows exactly what will run. Approve, and
  the agent's identical call runs once.
- **Credentials:** they reach the secret store, Infisical, only through
  a one-time form, created with
  `sesh credential request <connector> <NAME>`. Only the gateway can
  read them.
- **Your phone:** run `tailscale serve` for ports 7002 (approvals) and
  8090 (ntfy), and set `approvals_base_url` in `sesh.toml`. Then
  subscribe in the ntfy app as `person`, with `NTFY_PERSON_PASSWORD` from
  `secrets.env`. See
  [004's quickstart](specs/004-secrets-approvals/quickstart.md).

`sesh eval` runs a fixed set of tasks against a model through its alias.
Each run uses its own short-lived gateway with fake resources, so it
needs no internet and leaves the household's gateway and audit log
untouched. Results are kept under `~/.local/share/sesh/evals/`, for
`sesh eval list`, `compare` and `show`. Run it whenever you change the
model behind an alias.

## Repository

This repo is the project monorepo (code, deployment, operator skills,
specs). It never holds household data or per-user repositories, such as
agents' skills repos; those live on the server (ADR 0003).

## Development

Spec-driven, using [GitHub Spec Kit](https://github.github.com/spec-kit/):
constitution → specify → clarify → plan → tasks → implement. The `/speckit-*`
skills live in `.claude/skills/`, templates and scripts in `.specify/`.
