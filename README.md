# sesh

> *sesh* (Egyptian: "scribe"). The scribe writes; it doesn't hold the keys.

Run a coding agent on your own Mac, against your own local models, without
trusting it.

```bash
cd ~/code/some-repo
sesh
```

That one command:

1. brings up the local AI stack (Splash for inference, SearXNG for
   search) if it isn't running,
2. opens the repo in a **Docker Sandboxes (`sbx`) microVM**: deny-by-default
   network, no host files beyond an isolated copy of the repo, and its
   own Docker engine,
3. launches the **pi** harness inside it, wired to your local model.

When you're done, `sesh pull` brings the agent's commits back to your
checkout as a branch to review, as plain data. Nothing the agent wrote
runs on your Mac unless you choose to run it.

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
| [contracts/](specs/001-sesh-session/contracts/) | CLI, config schema, invariant suite |
| [quickstart.md](specs/001-sesh-session/quickstart.md) | The target user experience |
| [tasks.md](specs/001-sesh-session/tasks.md) | Ordered, test-first work items |

Decisions: [`docs/decisions/`](docs/decisions/).

## Requirements (planned)

macOS 26.4+ on Apple Silicon M3 or later, 48 GB+ memory recommended,
Homebrew.

## Development

Spec-driven, using [GitHub Spec Kit](https://github.github.com/spec-kit/)'s
layout: constitution → specify → clarify → plan → tasks → implement.
Spec Kit v1.4.2 is initialized for Claude Code: the `/speckit-*` skills
live in `.claude/skills/`, and templates and scripts in `.specify/`.
