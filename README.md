# sesh

> *sesh* (Egyptian: "scribe"). The scribe writes; it doesn't hold the keys.

Run the coding agent you already use, as easily as running it
directly, without trusting it.

```bash
cd ~/code/some-repo
sesh claude          # or codex, opencode, pi, or your own recipe
```

That one command:

1. opens the repo in a **Docker Sandboxes (`sbx`) microVM**: deny-by-default
   network, no host files beyond an isolated copy of the repo, and its
   own Docker engine,
2. brings in your own harness settings, skills and tools as a read-only
   copy, with credentials removed,
3. lets it reach only the endpoints you've allowed (your model provider
   or local inference server, registries, APIs), attaching credentials
   outside the sandbox.

When the harness exits, the agent's commits are back in your checkout as
branches to review, as plain data, with files that could run code
flagged. Nothing the agent wrote runs on your Mac unless you choose to
run it.

## Guarantees

The [constitution](.specify/memory/constitution.md) is binding. In short:

- The host is the only trusted zone; the agent and everything it writes
  are untrusted.
- Network and filesystem access are denied by default, and every
  allowance comes from *your* config, never from repo content.
- Secrets never enter the sandbox. They're attached on the host side,
  only for the hosts they belong to.
- Every guarantee is an executable check: `sesh verify`.

## Status

**Specification phase.** No code yet. Feature 001 is specified under
[`specs/001-sesh-session/`](specs/001-sesh-session/):

| Doc | What it holds |
|---|---|
| [spec.md](specs/001-sesh-session/spec.md) | User stories, requirements, success criteria |
| [research.md](specs/001-sesh-session/research.md) | Decisions, alternatives, evidence |
| [plan.md](specs/001-sesh-session/plan.md) | Architecture, constitution check, structure |
| [data-model.md](specs/001-sesh-session/data-model.md) | Entities, fields, state |
| [contracts/](specs/001-sesh-session/contracts/) | CLI, config schema, recipe format, invariant suite |
| [quickstart.md](specs/001-sesh-session/quickstart.md) | The target user experience |
| [tasks.md](specs/001-sesh-session/tasks.md) | Ordered, test-first work items |

Decisions: [`docs/decisions/`](docs/decisions/).

## Requirements (planned)

macOS 14+ on Apple Silicon (Linux planned),
Homebrew, and a terminal coding harness with an API key or a local
inference server.

## Development

Spec-driven, using [GitHub Spec Kit](https://github.github.com/spec-kit/)'s
layout: constitution → specify → clarify → plan → tasks → implement.
Spec Kit v1.4.2 is initialized for Claude Code: the `/speckit-*` skills
live in `.claude/skills/`, and templates and scripts in `.specify/`.
