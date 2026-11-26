# 0006. sesh becomes a local-first household agent platform

- **Status**: Accepted
- **Date**: 2026-11-26
- **Supersedes**: ADR 0003, ADR 0004; constitution 1.x (now 2.0.0)
- **Defers**: feature 001 (`specs/001-sesh-session/`) as "dev mode"

## Context

Through feature 001 sesh was specified as a tool for running a terminal
coding agent inside an sbx microVM. Research into sbx showed how thin
that tool would be over sbx itself. In parallel, the
[sesh Solutions Architecture](https://claude.ai/artifact/Hq9kuLsqLakFbAUW1Jyt5T)
document (with its [whiteboard](https://claude.ai/artifact/4HChjrjx4NPCuD1ktsM5LH))
set out a broader goal: always-on agents for each household member,
reaching every resource through one trusted gateway.

## Decision

sesh is the platform that document describes:

- **Assistant mode**: Hermes agents, one per user plus a Household
  agent, reached through Open WebUI and Telegram.
- **Control plane**: a Code Mode gateway (model-written scripts run in a
  workerd executor with no network or secrets), Infisical for secrets,
  ntfy for approvals, an append-only audit log.
- **Resources** behind the gateway: Google (OAuth), Mem0, Paperless-ngx,
  SearXNG, Kiwix, weather and maps, and a browser in sbx microVMs.
- **Inference**: oMLX on bare metal behind model aliases; no cloud models.
- **Operator mode**: the sesh admin CLI and Claude Code operator skills.
- Built in the document's seven phases, one Spec Kit feature each.

## Alternatives

- **Continue feature 001**: a useful tool, but mostly a hardening layer
  over sbx; kept as deferred dev mode.
- **Adopt a hosted agent platform**: against local-first and the
  household's data being the thing protected.

## Consequences

- The constitution is rewritten around the document's principles (2.0.0).
  "No sesh daemon" is removed; assistant mode is always-on.
- Feature 001's spec, plan and tasks stay in the repository, marked
  deferred; its sbx research still informs the browser phase.
- ADR 0001's sbx choice now applies to browser sessions; its isolated
  workspace applies only to dev mode. ADR 0002 is amended by ADR 0007.
  ADR 0005's platform and clone decisions apply only to dev mode.
