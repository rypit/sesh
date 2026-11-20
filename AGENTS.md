# Agent instructions

sesh is developed spec-first. Before changing anything:

1. Read `.specify/memory/constitution.md`. It is binding and overrides
   convenience.
2. Find the feature under `specs/NNN-*/`. Work from its `tasks.md`, in
   order, test-first: write an invariant or unit test and see it fail
   before implementing.
3. If a change touches a principle, a seam (`sandbox/sbx.rs`,
   `host/git.rs`, `host/launchd.rs`, `host/ports.rs`,
   `host/keychain.rs`), a built-in recipe's credential handling, or the
   threat model,
   write an ADR in `docs/decisions/`.

Hard rules:

- Never write secret values to any file, log, test fixture or commit.
- Never read repository content as sesh configuration (Principle V).
- Only `src/sandbox/sbx.rs` may invoke the `sbx` CLI.
- Keep `src/sandbox/policy.rs` and `src/scan.rs` pure (no `std::fs`,
  `std::process`, `std::net`).
- Harness-specific knowledge lives only in `recipes/` (data), never in
  Rust code.
- No `unsafe` code (`#![forbid(unsafe_code)]`).
- The project name is always lowercase, **sesh**, even at the start of a
  sentence or in a title.

sesh is written in Rust (ADR 0002). Commands (once code exists):
`make check` runs `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test` and `cargo deny check`. Invariant tests
(`cargo test --features invariants`) need a real Mac with sbx.
