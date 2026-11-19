# Agent instructions

Sesh is developed spec-first. Before changing anything:

1. Read `.specify/memory/constitution.md`. It is binding and overrides
   convenience.
2. Find the feature under `specs/NNN-*/`. Work from its `tasks.md`, in
   order, test-first: write an invariant or unit test and see it fail
   before implementing.
3. If a change touches a principle, a seam (`sandbox/sbx.py`,
   `stack/launchd.py`, the container-runtime call) or the threat model,
   write an ADR in `docs/decisions/`.

Hard rules:

- Never write secret values to any file, log, test fixture or commit.
- Never read repository content as Sesh configuration (Principle V).
- Only `src/sesh/sandbox/sbx.py` may invoke the `sbx` CLI.
- Keep `src/sesh/sandbox/policy.py` pure (no I/O).

Commands (once code exists): `make check` runs lint, typecheck and unit
tests. Invariant tests (`pytest -m invariants`) need a real Mac with sbx.
