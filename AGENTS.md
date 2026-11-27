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

The design source is `docs/architecture/solutions-architecture.md`
(ADR 0001); change it in the same pull request as the code it describes.
If a change adds, removes or renames a component, or changes what talks
to what, update the architecture diagram in the same change: follow
`docs/architecture/README.md` (`diagram.jsx` and `docs/architecture.svg`
stay identical).
Commands: `make check` runs fmt, clippy, unit tests, cargo-deny, the
architecture-diagram check, Compose validation and the instance-data
guard. `make stack-test` starts the stack with the stub model server and
runs the stack tests (needs Docker and the dev images:
`docker build -f deploy/<name>.Dockerfile -t sesh-<name>:dev .` for
`stub` (tagged `sesh-stub-llm:dev`), `gateway` and `executor`).
`sesh eval` runs the model-eval suite (feature 003); in stub mode,
`STUB_SCRIPT=pass sesh up` (or `fail`) makes the stub model play the
suite's scripted conversations.
`sesh credential request <connector> <NAME>` prints a one-time form link
for a credential (feature 004); nothing in sesh ever prints a value.
`sesh user add <name>` adds a household user; `sesh agent add … --owner
<name>` binds an agent to them, and `sesh google connect <name>` prints a
one-time Google consent link (feature 005). The gateway derives every
scope from the calling agent's owner, never from arguments.
`sesh feed add <owner> <url> [--name <name>]`, `remove` and `list`
manage feeds (feature 006); only the gateway fetches them, through
`connectors/feeds.rs` and its address filter (ADR 0009).
