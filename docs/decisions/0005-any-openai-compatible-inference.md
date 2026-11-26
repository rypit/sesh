# 0005. Inference: any OpenAI-compatible server, behind model aliases

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

Households will run different inference servers: oMLX or LM Studio on a
Mac, Ollama, llama.cpp or vLLM on other hardware. sesh should work with
whichever one is available, and its components (Hermes, Open WebUI,
Paperless-AI, Mem0) should not need changes when it changes.

## Decision

- sesh depends only on the **OpenAI-compatible API** (chat completions
  with tool calling, and model listing). No component names an engine.
- Every client calls a **model alias** (`agent-main`, and later others),
  configured once with the inference server's base URL.
- If the chosen server can't serve the alias names itself, a small
  router maps them: LiteLLM, already in the design as optional, becomes
  the standard way to do that, and adds per-agent keys and failover.
- The inference server stays on the home network (Principle VII). sesh
  assumes no particular server; none is installed or configured by
  default beyond the alias's base URL.
- `sesh doctor` checks that each alias answers and that tool calling
  works with a short probe.

## Alternatives

- **Standardize on oMLX**: fastest on Apple Silicon, but ties sesh to
  one engine and one platform.
- **Engine-specific adapters**: more control, more code; every engine
  sesh cares about already speaks the OpenAI-compatible API.

## Consequences

- Development and CI use a stub OpenAI-compatible server, or a small
  model on CPU, in the Compose stack; only model-quality checks need a
  real server.
- Tool-calling quality varies by engine and model; it is measured per
  setup, not assumed.
