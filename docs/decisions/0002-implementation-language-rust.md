# 0002. Implementation language: Rust

- **Status**: Accepted
- **Date**: 2026-11-19
- **Supersedes**: the Python 3.12 / `uv tool` choice in the constitution
  1.0.0 and research.md R8

## Context

The constitution 1.0.0 fixed the implementation language as Python 3.12+,
distributed as a `uv tool`. The user never chose that language; it was an
assumption made while drafting. No code exists yet, so changing it now
costs only documentation.

Two facts weigh on the choice:

- Most of sesh's code will be written by coding agents and reviewed by
  the user. The more defects the toolchain rejects before anything runs,
  the less that review has to catch.
- R8 said to revisit the language "if sesh ever owns a proxy or another
  long-running, security-critical process". FR-016 already requires one:
  the host-side credential shim, which holds real secrets and forwards
  requests to one upstream.

## Decision

sesh is written in **Rust** (stable, edition 2024, toolchain pinned in
`rust-toolchain.toml`), as a single Cargo package that builds one
`sesh` binary for `aarch64-apple-darwin`.

- Crates start minimal: `clap` (CLI), `serde` + `toml` (config, with
  `deny_unknown_fields` on every config type), `anyhow` (errors), and a
  small blocking HTTP client (`ureq`) for health checks. Anything
  heavier, such as an HTTP server and TLS stack for the shims (T043),
  is justified in the plan when it is added.
- `#![forbid(unsafe_code)]` crate-wide.
- Each external CLI seam (`sbx`, `launchctl`, the container runtime) is
  a trait with one real implementation that uses `std::process::Command`
  and a fake for unit tests. Only `src/sandbox/sbx.rs` invokes `sbx`.
- `src/sandbox/policy.rs` stays pure: no `std::fs`, `std::process` or
  `std::net`.
- Invariant probes run inside the Linux sandbox as POSIX shell scripts
  embedded in the binary, so the sandbox needs no Rust toolchain.
- Distribution: `cargo install --locked --git …` first, then a Homebrew
  tap with a prebuilt binary.

## Alternatives

- **Python 3.12 + `uv tool`** (the previous choice): fastest to write and
  iterate on, but more errors surface only at run time, and the shim
  would be a long-running Python process handling secrets.
- **Go**: single static binary and simple to read. Its type system is
  weaker for this domain (no sum types, nil values, errors that are easy
  to ignore).
- **Swift**: native to macOS, but sesh reaches every system facility
  through CLIs anyway, and the CLI/TOML ecosystem is thinner.

## Consequences

- Each edit-and-test cycle includes a compile, which is slower than
  Python. That's acceptable at sesh's size.
- Rust's guarantees cover memory and type errors, not logic. The policy
  compiler, the command construction at the seams and bundle handling
  still depend on unit tests, invariants and review.
- `make check` runs `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test` and `cargo deny check` (licence and advisory audit, which
  supports the minimal-dependencies constraint).
- Invariant tests run with `cargo test --features invariants` on the
  reference Mac.
- Constitution amended to 1.1.0 (Additional Constraints: Implementation).
