---
description: "Task list for feature 010: browser, with a live view and takeover in the chat"
---

# Tasks: Browser, with a live view and takeover in the chat

**Input**: `specs/010-browser/` (spec, plan, research, data-model, contracts)

**Tests**: required (constitution X). Each unit test is written failing before its code, and `browser_stack` before the Compose changes it checks.

---

## Phase 1: Setup

- [x] T001 [P] ADRs 0013 and 0014 (accepted 2026-11-28)
- [ ] T002 [P] `deploy/browser.Dockerfile`: pinned Debian base, Chromium, non-root user, a start script with the flags from research R3/R4 (`--headless=new`, remote debugging on the container's address, `--proxy-server`, the profile dir). Built in CI's `images` job as `sesh-browser:dev`.
- [ ] T003 [P] Dependencies (`tokio-tungstenite`, `hmac`, `sha1`) pass `cargo deny`.

## Phase 2: Foundational

- [ ] T004 Unit tests, then `browser/egress.rs`:
  - The address rules: RFC 1918, 100.64/10, 127/8, 169.254/16, ::1, fc00::/7, fe80::/10, IPv4-mapped IPv6, and 0.0.0.0. The `egress_allow` exception exists in stub mode only.
  - The proxy for HTTP and `CONNECT` connects to the checked address.
- [ ] T005 Unit tests, then `browser/totp.rs` (the RFC 6238 test vectors) and the site match for `login` (same host or subdomain, label-wise; lookalikes fail).
- [ ] T006 Unit tests, then `browser/cdp.rs`: the CDP client (request/response by id, events), against a fake WebSocket server.
- [ ] T007 Layout (`layout.rs`):
  - new: `browser-<agent>` and `chat`; removed: `agent-<agent>-chat`;
  - the gateway binds `:7003` on `chat` only and `:7004` on `browser-*` only (the alias rule from 009);
  - the rules tests are extended accordingly.

## Phase 3: US1 — read a page (P1) 🎯 MVP

- [ ] T008 Stack test (written first), `browser_stack` part 1:
  - `read` of the fake article returns Markdown, marked untrusted;
  - `read` of 10.0.0.1, 192.168.1.1, 100.100.100.100 and 127.0.0.1 is refused, and so is a public page that redirects to 10.0.0.1;
  - a subresource on 192.168.1.1 is refused (seen in the egress audit);
  - the size and time limits hold.
- [ ] T009 `browser/mod.rs`:
  - sessions per agent;
  - `read` with a fresh context and `page.js` extraction;
  - the connector is registered with the `browser` grant and its schema.
- [ ] T010 `up.rs` and `layout.rs`: the `browser-<agent>` service per agent granted `browser` (hardened, profile volume, one network), and the gateway's alias and listeners; `doctor` checks the browser is healthy and has no other network.
- [ ] T011 Stub `fake_sites.rs`: the article, a form, a login form, a redirect to 10.0.0.1, a subresource on 192.168.1.1. Hosted under network aliases of the stub.

## Phase 4: US3 — log in without seeing the password (P1)

- [ ] T012 Stack test part 2:
  - with `SITE_fake-bank.test` stored, `login` on the fake bank succeeds and its snapshot shows the password `[filled]`;
  - on `fake-bank.test.evil.test` it is refused, with nothing filled (checked on the stub's record of form posts);
  - the first login waits for approval;
  - no credential appears in any reply, the audit log or the logs.
- [ ] T013 `login`: credential lookup, site match, field discovery in `page.js`, `Input.insertText`, TOTP, the first-use gate, and the `browser_sites` table.

## Phase 5: US4 — browse step by step (P2)

- [ ] T014 Stack test part 3:
  - `open` then `snapshot` lists numbered elements with values masked;
  - `click(n)` follows a link;
  - `type` fills a search field;
  - a click on a submit button waits for approval;
  - a `type` with a line break is refused;
  - after restarting `browser-<agent>`, a cookie persists;
  - another agent's browser doesn't have it.
- [ ] T015 `open`, `snapshot`, `click` (with the submit gate) and `type`.

## Phase 5b: US5 — downloads and uploads (P1)

- [ ] T023 Unit tests, then `refs.rs`:
  - references gain a source kind (`GmailAttachment`, `Download`, `Document`) and a byte fetcher per kind;
  - `docs.upload` accepts every kind;
  - `docs.ref({id})` is scope-checked like `docs.get`.
- [ ] T024 Unit tests, then `browser/downloads.rs`:
  - the `Fetch`-domain capture: it detects attachments, enforces the size, type and count limits, discards partial files, and fails the request in the browser;
  - the store (memory, spilled to a gateway-owned directory, 24 h TTL).
- [ ] T025 `browser.upload` (gated, with the approval text from research R9) and `browser.downloads`; the page script sets the file with `DataTransfer`.
- [ ] T026 Stub fake sites: a download link (a PDF), an oversized download, a disallowed type, and an upload form that records what it received.
- [ ] T027 Stack test part 5:
  - a click on the download gives a ref and no bytes, and `docs.upload` of it files it in Paperless;
  - `browser.upload` of a Gmail attachment's ref waits for approval, and the fake form has nothing until the approval, then exactly those bytes;
  - another agent's ref is refused;
  - the oversized and disallowed downloads are refused;
  - nothing is left in the browser container's filesystem.

## Phase 6: US2 — live window in the chat, takeover (P1)

- [ ] T016 Unit tests, then `relay.rs`:
  - forwards to the runtime, with streaming passed through byte for byte;
  - checks the per-agent API key and the user JWT (HS256, expiry);
  - injects one `embeds` event chunk while the browser is in use, never into a non-streaming reply's content;
  - the model-facing content is untouched.
- [ ] T017 Unit tests, then `live.rs`:
  - grants (hashed secret, bound to user, agent and session, TTL);
  - the pages and their WebSocket: frames from screencast, input only from the control page while in control;
  - control state: the agent's calls wait and then fail;
  - hand-back on close or WebSocket drop; the takeover limit.
- [ ] T018 `up.rs`/`hermes.rs`:
  - Open WebUI's runtime connections point at `http://gateway:7003/<agent>/v1`;
  - `ENABLE_FORWARD_USER_INFO_HEADERS` and `FORWARD_USER_INFO_HEADER_JWT_SECRET`, with the secret generated by `sesh init`/`up`;
  - `agent_stack` updated for the new path.
- [ ] T019 Stack test part 4 (`browser_stack`, with the runtime and `STUB_SCRIPT=agent`):
  - `[agent:browse]` opens the fake article, and the Open WebUI message has an embed URL on the approvals listener;
  - the view WebSocket with the fragment secret receives a JPEG frame;
  - taking over makes the agent's next browser call wait, then fail with "the person is in control";
  - input from the control page changes the page;
  - closing hands back, and the next call succeeds;
  - a grant used with another user's JWT, a wrong secret or after expiry is refused (SC-005);
  - text typed in a takeover never appears in the agent's replies (SC-003);
  - a file chosen in the control page reaches the fake form, and a download during the takeover shows up in `browser.downloads`.

## Phase 7: Polish

- [ ] T020 [P] Architecture doc (Browser row: container, not sbx; drop sbx from Deployment, Adopt list, risks and references; Security, Network layout, phase 10) and the diagram (the browser connects only to the gateway; Open WebUI connects to the gateway); README status; AGENTS.md.
- [ ] T021 [P] CI: build `sesh-browser:dev` in `images`; add `browser_stack` (the matrix guard test enforces it).
- [ ] T022 Record CI results and deviations (Chromium's sandbox on the runner) in research.md.

## Dependencies

- T004–T007 come before everything else; T011 comes before T008's run.
- US1 → US3 → US4 → US5 → US2: US2 builds on sessions and the screencast; US5 needs `click` (US4).
