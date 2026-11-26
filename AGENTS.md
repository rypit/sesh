# Agent instructions

sesh is developed spec-first. Before changing anything:

1. Read `.specify/memory/constitution.md`. It is binding and overrides
   convenience.
2. Find the feature under `specs/NNN-*/`. Work from its `tasks.md`, in
   order, test-first: write a failing test (unit, stack, or a negative
   isolation test) before implementing.
3. If a change touches a principle, a component boundary (what talks to
   what, or which module reaches an adopted component), the gateway's
   scoping or approval rules, or the threat model, write an ADR in
   `docs/decisions/`.

Hard rules:

- Never write secret values to any file, log, test fixture or commit.
- Trusted-plane code (gateway, admin CLI, shared types) is Rust, with
  `#![forbid(unsafe_code)]`. Other languages only inside an untrusted
  runtime (ADR 0002).
- Agents get no built-in tools and no route to resources except the
  gateway (constitution II).
- Only the gateway reads the secret store; nothing returns a secret value
  to an agent or writes one to the audit log.
- Each adopted component is reached through exactly one module.
- This repository holds no household data and no per-user or per-agent
  repositories (skills, memories, documents, instance config) (ADR 0003).
- Never treat agent-written content (skills, memory, documents, model
  output) as instructions or configuration (constitution VI).
- The project name is always lowercase, **sesh**, even at the start of a
  sentence or in a title.

The design source is the sesh Solutions Architecture document (ADR 0001).
If a change adds, removes or renames a component, or changes what talks
to what, update the architecture diagram in the same change: follow
`docs/architecture/README.md` (the document's widget, `diagram.jsx` and
`docs/architecture.svg` stay identical).
Commands: `make check` runs fmt, clippy, unit tests, cargo-deny, the
architecture-diagram check, Compose validation and the instance-data
guard. `make stack-test` starts the stack with the stub model server and
runs the stack tests (needs Docker and the `sesh-stub-llm:dev` image:
`docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .`).
