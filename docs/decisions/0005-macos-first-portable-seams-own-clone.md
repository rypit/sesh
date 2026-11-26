# 0005. macOS first with portable seams; sesh's own clone over `sbx --clone`

- **Status**: Accepted for deferred dev mode only (ADR 0006)
- **Date**: 2026-11-20
- **Amends**: ADR 0003 (service manager naming), ADR 0004 (seam names);
  constitution 1.2.0 (Platform, No sesh daemon, Principle III wording)

## Context

Reading the Docker Sandboxes docs at their source (docker/docs,
`content/manuals/ai/sandboxes/`, sbx 0.54.0) showed two things:

1. sbx runs on macOS 14+ (Apple Silicon), Windows 11 and Ubuntu 24.04+.
   sesh's "macOS only" constraint came from Splash, which ADR 0004
   removed. What remains macOS-specific in sesh is small: reading
   credential sources from the Keychain (`security`), running shims
   under launchd, and a few invariant paths (`~/Library`).
2. `sbx run --clone` gives the agent its own Git clone inside the
   sandbox, but mounts the host repository read-only at
   `/run/sandbox/source`.

## Decision

1. **Platform**: macOS on Apple Silicon first; Linux planned, Windows
   not planned. OS-specific code lives only behind two seams named for
   their role: `host/secret_store.rs` (macOS: `security`) and
   `host/service_manager.rs` (macOS: launchd). Linux later adds
   implementations (libsecret `secret-tool`, systemd user units) and an
   invariant run, without changing other modules.
2. **Workspace**: sesh keeps its own isolated clone and passes its
   directory to sbx as the workspace; it does not use `--clone`. The
   clone has tracked files plus untracked, non-ignored ones, and its
   remotes carry no embedded credentials.
3. **Built-in secret services first**: recipes use sbx's built-in
   services (anthropic, openai, github, …) where they exist, including
   their host-side OAuth logins, and `set-custom` or shims otherwise.

## Alternatives

- **Stay macOS-only**: no cost now, but names like `keychain.rs` and
  `launchd.rs` would leak into callers and make Linux a refactor.
- **Ship Linux in feature 001**: doubles invariant runs and needs a
  second reference machine, for a user who works on a Mac.
- **Use `sbx --clone`**: less code, but the read-only mount still shows
  the agent ignored files (`.env` secrets) and `.git/config`
  (credentials in remote URLs), against Principle III; and its return
  path is undocumented.

## Consequences

- Constitution 1.2.0 text changes: Platform says "macOS first, Linux
  planned"; the no-daemon constraint and Principle III name the
  platform's service manager and credential store generically. (Folded
  into 1.2.0, which hasn't been released.)
- `keychain:` stays the macOS credential-source scheme; Linux adds its
  own scheme when supported.
- INV-F1's `~/Library` path becomes platform-specific when Linux lands.
