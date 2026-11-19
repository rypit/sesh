# 0003. Credential shims are launchd stack services in a separate binary

- **Status**: Accepted
- **Date**: 2026-11-19
- **Amends**: ADR 0002 (one binary → two)

## Context

FR-016 requires host-side shims for APIs whose auth sbx can't inject
(query parameters, HTTP Basic, request signing). A shim runs for as long
as the stack does and holds real credentials. The constitution forbids a
sesh daemon, and SC-007 says sesh leaves no resident process of its own.
The documents disagreed on what a shim is: plan.md built shims into the
`sesh` crate, and contracts/config.md started one as `sesh-shim-weather`.

## Decision

- A shim is a **stack service**, like the engine and search: launchd
  starts it, keeps it alive and stops it (`sesh stack down`). The `sesh`
  CLI only writes its launchd job and never runs as the shim.
- Shims ship as a second binary, **`sesh-shim`**, built from the same
  Cargo package (`src/bin/sesh-shim.rs`). One shim per process:
  `sesh-shim <name>`, one launchd job per configured shim.
- The HTTP server and TLS client crates a shim needs are dependencies of
  `sesh-shim` only, and are justified in plan.md when T043 adds them.
- Each shim binds to `127.0.0.1` only, reads its credential from the
  Keychain per request, and forwards only to its one upstream on its
  allowed routes (research.md R2).

## Alternatives

- **A `sesh shim <name>` subcommand of the CLI**: one binary, but the
  resident process would be "sesh" itself, blurring SC-007, and the CLI
  would carry server and TLS code.
- **One binary per shim** (`sesh-shim-weather`): even smaller processes,
  but more build and packaging work for no added isolation, since each
  shim is already its own process with its own credential.

## Consequences

- The constitution's "No sesh daemon" constraint names shims explicitly
  (constitution 1.1.1).
- `sesh stack status` lists shims, and the invariants check that only
  launchd-managed `sesh-shim` processes remain with no session open
  (INV-P4), and that every stack port is loopback-only (INV-N8).
- A compromised shim exposes only its own credential and upstream.
