# 0002. Languages by trust zone: Rust for the trusted plane

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

sesh writes or configures five pieces of code: the gateway, the admin
CLI, the script executor, a Hermes memory plugin and a browser
controller. The gateway is the one long-running process that holds every
agent's tokens, reads the secret store and fills logins. The executor
runs model-written scripts and is untrusted (ADR 0004).

## Decision

| Piece | Runs in | Language |
|---|---|---|
| sesh-gateway | trusted control plane | **Rust** |
| sesh admin CLI, shared types | operator's host account | **Rust** |
| executor | workerd, one per agent (no network, no secrets) | JavaScript: the adopted `@cloudflare/codemode` runtime plus thin connector stubs that forward every call to the gateway |
| hermes-memory-sesh | inside Hermes | Python (Hermes plugin interface) |
| sesh-browser | inside the sbx browser VM | adopt an existing controller; else TypeScript + Playwright |

Rule: trusted-plane code is Rust (stable, toolchain pinned, `unsafe`
forbidden); code inside an untrusted runtime uses what that runtime
requires.

## Alternatives

- **TypeScript gateway on `@cloudflare/codemode`**: the runtime would
  sit in the same workerd process as model-written scripts, where a V8
  isolate is the only boundary, so secrets and approval authority would
  sit next to untrusted code. Rejected.
- **Code Mode's runtime rebuilt in Rust**: its durable call log,
  abort-and-replay approvals, divergence detection, rollback, snippets,
  search and type generation are a substantial project; adopting them
  inside the untrusted executor costs nothing in trust, because the
  gateway re-checks everything (docs/research/codemode.md).
- **Rust everywhere**: Rust browser-control crates are far less mature
  than Playwright, and the browser controller is on the untrusted side.

## Consequences

- The gateway is the MCP server agents talk to and the only holder of
  authority; the executor's design is in ADR 0004.
- `@cloudflare/codemode` is experimental (0.5.x); sesh pins its version
  and re-runs the executor tests on every upgrade.
- One Cargo workspace (`crates/`) holds the gateway, CLI and shared types.
