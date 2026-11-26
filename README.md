# sesh

> *sesh* (Egyptian: "scribe"). The scribe writes; it doesn't hold the keys.

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
an agent. The diagram is generated from the Solutions Architecture
document; see [`docs/architecture/`](docs/architecture/) to update it.

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

**Planning.** No code yet. The design is in the sesh Solutions
Architecture document; decisions are in [`docs/decisions/`](docs/decisions/)
and reference notes in [`docs/research/`](docs/research/). The build
follows the architecture's seven phases, each a feature under `specs/`.

## Repository

This repo is the project monorepo (code, deployment, operator skills,
specs). It never holds household data or per-user repositories, such as
agents' skills repos; those live on the server (ADR 0003).

## Development

Spec-driven, using [GitHub Spec Kit](https://github.github.com/spec-kit/):
constitution → specify → clarify → plan → tasks → implement. The `/speckit-*`
skills live in `.claude/skills/`, templates and scripts in `.specify/`.
