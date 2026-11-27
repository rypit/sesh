# 0001. sesh is a local-first household agent platform

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

A household wants AI agents that act for each person (email, calendar,
documents, browsing, errands) without trusting those agents with
credentials or unchecked side effects, and without sending household data
to cloud model providers. The design is set out in the
[sesh Solutions Architecture](https://claude.ai/artifact/Hq9kuLsqLakFbAUW1Jyt5T)
document and its [whiteboard](https://claude.ai/artifact/4HChjrjx4NPCuD1ktsM5LH).

## Decision

sesh is the platform that document describes:

- **Assistant mode**: Hermes agents, one per user plus a Household
  agent, reached through Open WebUI and Telegram.
- **Control plane**: the gateway, Infisical for secrets, ntfy for
  approvals, an append-only audit log.
- **Executor**: model-written scripts run in a workerd executor per
  agent, with no network or secrets, using the Code Mode runtime; it is
  untrusted and every call it makes goes through the gateway
  (ADR 0004).
- **Resources** behind the gateway: Google (OAuth), Mem0, Paperless-ngx,
  SearXNG, Kiwix, weather and maps, and a browser in sbx microVMs.
- **Inference**: any OpenAI-compatible server the operator runs on the
  home network, behind model aliases; no cloud models (ADR 0005).
- **Operator mode**: the sesh admin CLI and Claude Code operator skills.
- Built in the document's phases, one Spec Kit feature each.

## Alternatives

- **A hosted agent platform**: against local-first; the household's data
  is the thing being protected.
- **Agents with built-in tools and prompt-level rules**: rules in prompts
  don't hold against injected content; isolation must sit outside the
  agent.

## Consequences

- The constitution is built around the document's principles.
- The control plane is the only trusted zone; every guarantee about it
  needs an automated check, including negative isolation tests.
- sbx is used for browser sessions, where untrusted web content runs.
