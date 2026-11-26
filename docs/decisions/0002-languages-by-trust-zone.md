# 0002. Languages by trust zone: Rust for the trusted plane

- **Status**: Accepted
- **Date**: 2026-11-26

## Context

sesh writes four pieces of code: the gateway, the admin CLI, a Hermes
memory plugin and a browser controller, plus a small shim inside the
workerd script executor. The gateway is the one long-running process that
holds every agent's tokens, reads the secret store and fills logins.

## Decision

| Piece | Runs in | Language |
|---|---|---|
| sesh-gateway | trusted control plane | **Rust** |
| sesh admin CLI, shared types | operator's host account | **Rust** |
| executor shim | inside workerd (no network) | JavaScript, minimal: forwards connector calls only |
| hermes-memory-sesh | inside Hermes | Python (Hermes plugin interface) |
| sesh-browser | inside the sbx browser VM | adopt an existing controller; else TypeScript + Playwright |

Rule: trusted-plane code is Rust (stable, toolchain pinned, `unsafe`
forbidden); code inside an untrusted runtime uses what that runtime
requires.

## Alternatives

- **TypeScript gateway on `@cloudflare/codemode`**: less glue, since type
  generation and tool-call bridging come with the library. Not chosen
  because the gateway is where compile-time checks matter most.
  **Revisit** if the codemode prototype shows the library provides
  substantially more than type generation and call bridging (e.g. a
  durable runtime sesh needs).
- **Rust everywhere**: Rust browser-control crates are far less mature
  than Playwright, and the browser controller is on the untrusted side.

## Consequences

- The gateway implements codemode's glue itself: an MCP server (the
  official Rust SDK), generated TypeScript declarations for each token's
  connectors, and a socket protocol to the workerd executor.
- One Cargo workspace (`crates/`) holds the gateway, CLI and shared types.
