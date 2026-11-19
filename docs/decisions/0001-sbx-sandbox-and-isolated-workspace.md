# 0001. sbx microVM sandbox with an isolated workspace

- **Status**: Accepted
- **Date**: 2026-11-19

## Context

sesh runs an untrusted, possibly prompt-injected coding agent on the
user's Mac. The main goal is that the agent can't harm the host: steal
secrets, exfiltrate beyond an allowlist, or get code executed on the
host. Inference and voice models must run natively (Metal isn't
available inside Linux VMs), so the host keeps those services, and only
the agent is confined.

## Decision

1. Agents run in **Docker Sandboxes (`sbx`)** microVMs, one per
   repository, under a deny-by-default policy compiled from user config.
   Credentials are attached by sbx's host-side proxy (placeholders inside
   the VM) or by host shims for auth formats sbx can't inject.
2. The sandbox's workspace is an **isolated clone** of the user's
   checkout. The host checkout's `.git` is never exposed. Work returns as
   a git bundle created inside the sandbox and fetched on the host.

## Alternatives

- Hardened Docker container + host mitmproxy (fallback; shared VM, no
  safe Docker for the agent, more to build).
- Anthropic `srt` (Seatbelt, no VM; shared kernel).
- microsandbox, Strands Box (promising, beta/preview).
- Mounting the checkout directly (rejected: `.git/config` and hooks let
  agent-written content run on the host).

See `specs/001-sesh-session/research.md` R1, R2 and R6.

## Consequences

- Depends on Docker's sbx, including experimental custom secrets; the
  sbx version is pinned and the invariants catch regressions.
- Known gap: sbx substitutes placeholders in any header
  (docker/sbx-releases#707), tracked as a KNOWN-RISK invariant.
- Reviewing agent work means `sesh pull`, then git on the host. The user
  must not run agent-written code on the host before reviewing it.
