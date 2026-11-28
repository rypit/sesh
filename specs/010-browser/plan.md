# Implementation Plan: Browser, with a live view and takeover in the chat

**Branch**: `claude/browser` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

## Summary

- **Browser**: Chromium in a hardened container per agent
  (`browser-<agent>`), on one internal network shared only with the
  gateway; its profile in a per-agent volume (research.md R3).
- **Egress**: all of the browser's traffic goes through the gateway's
  egress proxy (`:7004`), which refuses home-network addresses after
  resolving (R4).
- **Control**: the gateway drives the browser over CDP from one module.
  The `browser` connector offers `read`, `open`, `snapshot`, `click`,
  `type` and `login`; form submits and first logins are gated (R5).
- **Chat path**: Open WebUI's connection to each agent runtime goes
  through the gateway's chat listener (`:7003`), which forwards to the
  runtime and adds a live-window embed to the message when the browser
  is in use (R1, R2).
- **Live view**: pages on the approvals listener (`:7002`). A small
  view-only window sits in the message, with a larger window to take
  over. Frames and input pass through the gateway; grants are
  short-lived and bound to the Open WebUI user (R6).
- **Site logins**: stored through the credential form, filled by the
  gateway only on the saved site (R7).
- **ADRs**: 0013 (the browser, its isolation and egress) and 0014 (the
  gateway in the chat path, and the live view). Plus the architecture
  text and diagram.

## Technical Context

**Language/Version**: Rust 1.97 (`#![forbid(unsafe_code)]`); a page
script and the live-view page are static assets compiled into the
gateway; Compose YAML; a Dockerfile for the browser image.

**Primary Dependencies**:
- `tokio-tungstenite` (a CDP client and the live view's WebSocket);
- `hmac` and `sha1` (TOTP; RFC 6238);
- `jsonwebtoken` or a small HS256 check with `hmac` + `sha2` (Open
  WebUI's user header).

`cargo deny` vets each.

**Storage**:
- gateway Postgres tables for live-view grants (hashed secrets) and
  approved first logins;
- a volume per agent for the browser profile.

**Testing**:
- unit tests: the egress address rules, the domain match for `login`,
  TOTP vectors, the snapshot masking (against fixtures), the relay's
  embed injection and JWT check, the overlay and the layout;
- stack tests: `browser_stack` (new), plus `agent_stack` and
  `network_stack`.

**Constraints**:
- no internet in CI, so fake sites come from the stub;
- the browser image is built in CI like the other dev images.

**Scale/Scope**: one browser per agent; one live view per session.

## Constitution Check

| Principle | Check |
|---|---|
| I. Only the control plane is trusted | The browser is untrusted: one network to the gateway, no route out, hardened. The gateway's checks (egress, domain, masking) run in the gateway. |
| II. One way out | The agent reaches the browser only through the gateway; the browser reaches the web only through the gateway's egress proxy. |
| III. Agents never see secrets | Site passwords and TOTP seeds are read and filled by the gateway; the agent gets success or failure; fields are masked in snapshots. |
| IV. Per-user isolation | A profile per agent; live view grants per Open WebUI user; site logins per owner. |
| V. Side effects need approval | Form submits and first logins per site are gated. |
| VI. Agent content is data | Page text is labelled untrusted; no Open WebUI functions; the agent supplies no script. |
| VII. Local-first | Nothing new leaves the house; the browser goes to the sites the person asked for. |
| VIII. Adopt before build | Chromium is adopted; the CDP driver is built, small, because the checks must be in the trusted plane (research R5). |
| X. Every guarantee is executable | Each FR has a unit or stack test, including the negative cases. |

## Project Structure

```text
crates/sesh-gateway/src/browser/mod.rs      the connector (read/open/snapshot/click/type/login), sessions, control
crates/sesh-gateway/src/browser/cdp.rs      CDP client
crates/sesh-gateway/src/browser/page.js     the gateway's page script (snapshot, extract, field lookup)
crates/sesh-gateway/src/browser/egress.rs   egress proxy and address rules
crates/sesh-gateway/src/browser/live.rs     grants, live pages, screencast and input WebSocket
crates/sesh-gateway/src/browser/totp.rs     TOTP
crates/sesh-gateway/src/relay.rs            chat listener: forward to the runtime, add the embed, check the user JWT
crates/sesh-gateway/src/listen.rs           chat and egress listeners' bindings
crates/sesh-cli/src/up.rs, layout.rs, hermes.rs, doctor.rs   browser service, networks, Open WebUI → gateway
crates/sesh-stub-llm/src/fake_sites.rs      fake sites
deploy/browser.Dockerfile, deploy/compose.yaml, deploy/postgres/gateway.sql
docs/decisions/0013-browser.md, 0014-chat-relay-live-view.md
```
