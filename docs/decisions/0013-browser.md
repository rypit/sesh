# 0013. The browser: a container per agent, driven by the gateway, online only through the gateway

- Status: accepted
- Date: 2026-11-28
- Feature: specs/010-browser

## Context

Agents need to read pages, browse step by step and log in to sites.
The architecture planned a Chromium in an sbx microVM per session, with
a TypeScript controller inside it. Two facts changed that:
- sbx requires a Docker sign-in and isn't available on CI's Linux
  runners;
- a controller inside the browser's sandbox would hold site passwords
  and make the domain check for `login`, in untrusted code.

## Decision

1. **Chromium in a hardened container per agent** (`browser-<agent>`),
   from a sesh-built image on a pinned Debian base:
   - read-only root, tmpfs, no capabilities, `no-new-privileges`, memory
     and process limits;
   - its profile in a per-agent volume;
   - Chromium's own sandbox stays on.

   No microVM: the household decided the hardened container is enough
   (2026-11-28), which also drops sbx and its Docker sign-in.
2. **One internal network**, `browser-<agent>`, shared only with the
   gateway. The browser has no route anywhere else.
3. **Online only through the gateway's egress proxy** (`:7004`). The
   gateway resolves each host and refuses private, CGNAT and Tailscale,
   loopback, link-local, unique-local and metadata addresses, then
   connects to the address it checked. This covers navigation,
   redirects, subresources and WebSockets alike.
4. **The gateway drives the browser over CDP**, in one module. The page
   scripts are the gateway's own; the agent supplies none.
   - Snapshots never include field values, and password fields are
     always masked.
   - `login(site)` checks the browser's current host against the
     credential's site, fills the fields itself (TOTP included), and
     reports success or failure.
   - Form submits, uploads, and the first login per owner and site are
     gated.
5. **Downloads stay in the gateway.**
   - The gateway captures downloads over CDP and keeps them outside the
     browser.
   - The agent gets a file reference, like a mail attachment's, usable
     with `docs.upload` and `browser.upload`.
   - Uploads put a referenced file into a page's file field after the
     person approves.
   - In neither direction do the bytes reach the model.

## Alternatives

- **An sbx microVM**: needs a Docker account sign-in and can't be
  tested in CI; not needed for the household's threat model. Rejected.
- **A Playwright controller inside the sandbox** (the architecture's
  `sesh-browser`): it would put credentials and the domain check in
  untrusted code. Rejected.
- **Adopting Auto Browser, easi.run or Nod**: each adds a third-party
  server between the gateway and the browser. Not adopted for now.
- **Filtering with host firewall rules** instead of a proxy: these can't
  see DNS names or redirects, and they vary by host. Rejected.

## Consequences

- **Threat model**: a page that compromises Chromium inside the
  container can reach only the gateway's CDP client, the egress proxy
  and its own profile. It can't reach another agent's profile, the
  household's services or the home network.
- **New dependencies**: a WebSocket client and TOTP primitives in the
  gateway; one more image built in CI.
- **Resources**: each running browser costs about 300–500 MB of memory.
