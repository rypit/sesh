# 0004. Harness-agnostic recipes and user-declared endpoints

- **Status**: Accepted
- **Date**: 2026-11-19
- **Supersedes**: research.md R3–R7 (Splash, oMLX, pi as the harness,
  SearXNG on the host, the container runtime for host services) and the
  "stack" of the first feature 001 spec
- **Constitution**: 1.2.0

## Context

The first feature 001 spec had sesh bring up a local AI stack (Splash,
SearXNG) and launch one harness (pi). Working through it showed that:

- sesh's distinctive value is the boundary: policy compiled only from
  user config, secrets kept out of the sandbox, the isolated workspace,
  work returned as data, and `sesh verify`. None of that depends on
  which harness runs or who serves inference.
- Running and supervising inference and search duplicates what other
  tools (e.g. Sovereign, Ollama, LM Studio) already do, against
  Principle VII.
- The goal is "as ergonomic as running `claude`, but secure", for any
  terminal harness, with the user's own tools and configuration.
  Local inference is one option, not the point.

## Decision

1. **Recipes.** All harness knowledge is data: install source (sbx kit
   or image), launch command, host config paths to import, files that
   hold credentials, the endpoints and credentials the harness needs,
   and how it learns where its endpoints are. sesh ships recipes for
   Claude Code, Codex CLI, opencode and pi; users add their own in their
   config directory. Recipes have `workspace` and `attach` fields;
   feature 001 implements only `repo` and `terminal`.
2. **Config import.** A recipe's host config paths are copied into the
   sandbox per session as a snapshot. Credential files and
   high-confidence secret matches are removed, and nothing is synced
   back, so the agent can't plant hooks in the user's real harness
   config.
3. **Endpoints.** Everything the sandbox may reach is an endpoint:
   host or local port, method/path rules, credential bindings, an
   optional start command. sesh never installs or supervises inference
   or search. Local endpoints must be loopback-only.
4. **Allowlist ergonomics.** sesh lists refused hosts after each session;
   `sesh allow <host>` writes a repository-scoped endpoint (or global
   with `--global`) to user config.
5. **Automatic return.** Work returns as a git bundle when the harness
   exits, with files that can run code implicitly highlighted.

## Alternatives

- **Keep the local stack in sesh**: one command for everything, but
  duplicates stack managers and ties sesh to specific engines.
- **Harness adapters in code**: richer integration per harness, but every
  new harness needs a sesh release, and the code grows with each one.
- **No harness knowledge at all** (just run a command): simplest, but
  then the user's config and login can't come with them safely, which
  defeats the ergonomics goal.
- **Mount the user's harness config read-write**: most seamless, but
  lets the agent plant hooks that run on the host the next time the
  harness runs outside sesh (Principle IV).

## Consequences

- sesh no longer pins Splash or pi versions; only the sandbox runtime.
- The platform minimum comes from sbx, not from an inference engine.
- Subscription logins work only if sbx can attach them outside the
  sandbox; otherwise sesh reports them as unsupported (spike).
- Recipes must be maintained as harnesses move their config files.
- research.md, plan.md, tasks.md, quickstart.md and contracts/ are
  regenerated for the new spec.
- Seams change: the container-runtime seam and `stack/launchd.rs` go;
  the seams are `sandbox/sbx.rs`, `host/git.rs`,
  `host/service_manager.rs` (shim jobs), `host/ports.rs` (`lsof`) and
  `host/secret_store.rs` (renamed from launchd/keychain by ADR 0005).
- Future workspace kinds (volume) and attach modes (detached), e.g. for
  always-on agents, fit the recipe format without being built now.
