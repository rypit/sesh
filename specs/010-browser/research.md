# Research: Browser, with a live view and takeover in the chat

Sources: Open WebUI's source at tag `v0.13.5` (raw GitHub, 2026-11-28):
`backend/open_webui/utils/middleware.py`, `backend/open_webui/socket/main.py`,
`backend/open_webui/env.py`, `backend/open_webui/utils/headers.py`,
`src/lib/components/chat/Messages/ResponseMessage.svelte`,
`src/lib/components/common/FullHeightIframe.svelte`,
`src/lib/components/chat/Messages/MCPApp.svelte`; Docker's Sandboxes
docs (docker/docs, `content/manuals/ai/sandboxes/get-started.md`).
docs.openwebui.com is not reachable from the build environment.

## R1. How a live window gets into an Open WebUI message

**Finding**:
- **Message embeds.** Open WebUI shows *embeds* under an assistant message
  (`ResponseMessage.svelte` ~836). A string embed goes into a
  `FullHeightIframe` (`src` if it is a URL, `srcdoc` if HTML), sandboxed
  with `allow-scripts`, `allow-forms` and `allow-popups`, and *without*
  `allow-same-origin` by default. A cross-origin page sizes itself by
  posting `{type: 'iframe:height', height}` to its parent.
- **Embeds from an upstream stream.** While Open WebUI streams from an
  OpenAI-compatible connection, any SSE chunk with an `event` key is
  passed to its event emitter (`middleware.py` ~5029:
  `if 'event' in data: await event_emitter(data.get('event'))`). The
  `embeds` event type is saved onto the message
  (`socket/main.py` ~1713) and pushed to the browser. A chunk with no
  `choices` and no `error` is otherwise ignored.
- **Embeds aren't sent back.** They are message metadata, not content, so
  they are never part of the next request: the model never sees the URL.
- **MCP Apps** (`type: 'mcp_app'` embeds) exist but need Open WebUI's own
  MCP connection to a tool server, which our agents don't use (the
  runtime runs its tools itself, feature 009).

**Decision**: the chat request goes from Open WebUI through the gateway
to the runtime (R2), and when the agent's browser is in use the gateway
adds one SSE chunk to the stream:

```
data: {"event": {"type": "embeds", "data": {"embeds": ["https://<approvals base>/live/<grant>#<secret>"]}}}
```

Open WebUI is not modified and holds no sesh code (constitution VI).

**Alternatives**:
- An Open WebUI filter or pipe function that injects the embed: code
  stored in Open WebUI's database. Rejected (constitution VI).
- A link in the model's answer: the model would see the token, and the
  person would have to click a link. Rejected (FR-011, the household's
  requirement).
- A tool server (OpenAPI or MCP) that Open WebUI calls: Open WebUI would
  run the agent's browser tools, not the agent. Rejected.

## R2. The gateway in the chat path ("chat relay")

**Decision**: Open WebUI's connection for each agent runtime points at
the gateway's new **chat listener** (`:7003`), which checks the
per-agent API key and forwards the request, unchanged, to
`hermes-<agent>:8642` over `agent-<agent>`, streaming the reply back.
- It adds the live window's embed (R1) when that agent's browser
  session is in use during the request.
- It reads the Open WebUI user from a signed header: Open WebUI sends
  an HS256 JWT (`X-OpenWebUI-User-Jwt`) when
  `ENABLE_FORWARD_USER_INFO_HEADERS=true` and
  `FORWARD_USER_INFO_HEADER_JWT_SECRET` are set (`headers.py`
  `_mint_forward_user_jwt`; `sub` is the user id, valid 300 s). The
  gateway checks it with the shared secret. The live view grant is for
  that user.
- Network: a new pair network `chat` (Open WebUI and the gateway),
  where the chat listener binds (ADR 0011's exact-binding rule). The
  feature 009 network `agent-<agent>-chat` goes away: Open WebUI no
  longer reaches the runtime at all.

**Rationale**: the gateway is the only trusted component that knows
both the chat (who is asking, in which request) and the browser
session. It is already on the runtime's network.

**Alternatives**: a separate relay service (another trusted component
with the same knowledge, for no gain); polling from the live page (no
way to get the window into the message). Rejected.

## R3. Isolation: a container per agent, not an sbx microVM

**Finding**: Docker Sandboxes (`sbx`) run coding agents in microVMs,
need the `sbx` CLI and a Docker sign-in, and are not available on CI's
Linux runners. The architecture already records the sign-in as a risk.

**Decision**: a hardened container per agent, `browser-<agent>`, built
by sesh (`deploy/browser.Dockerfile`, Debian Chromium, pinned base):
- the same hardening as executors and the runtime (read-only root,
  tmpfs, `cap_drop: ALL`, `no-new-privileges`, memory and process limits);
- the profile in a per-agent volume; Chromium's own sandbox on (it needs
  user namespaces; if the runner refuses, the stack test records it and
  the seccomp profile is adjusted, never `--no-sandbox` silently).

The household decided (2026-11-28) that no microVM is needed: the
hardened container is the isolation, and sbx is dropped from the
architecture.

## R4. No route out except through the gateway's egress check

**Decision**:
- **Networks**: the browser container is on one internal pair network,
  `browser-<agent>` (with the gateway), and nothing else, so it has no
  route anywhere.
- **Proxy**: Chromium runs with `--proxy-server=http://gateway:7004` and
  `--proxy-bypass-list=<-loopback>`. The gateway's **egress proxy**
  (`:7004`, bound on the `browser-*` networks only) handles plain HTTP
  and `CONNECT`:
  - it resolves the host itself;
  - it refuses private (RFC 1918, RFC 6598 CGNAT including Tailscale's
    100.64/10), loopback, link-local, unique-local and metadata
    addresses;
  - it connects to the address it checked, so DNS rebinding can't
    switch it.
- **Coverage**: navigation, redirects, subresources and WebSockets all
  go through it (FR-003). Non-HTTP traffic (WebRTC UDP, other ports)
  has no route at all.

## R5. Driving the browser: the gateway speaks CDP

**Decision**: the gateway drives Chromium over the Chrome DevTools
Protocol (`--remote-debugging-address` on the `browser-<agent>`
network, port 9222), in one module, `crates/sesh-gateway/src/browser/`.
It is a small CDP client over a WebSocket (`tokio-tungstenite`, a new
dependency).
- **The agent's calls map to the gateway's own CDP calls.** The page
  scripts the gateway evaluates are its own, fixed in the binary; the
  agent never supplies script (FR-005).
- **`snapshot`**: visible text, plus numbered links, buttons and fields.
  A field's value is never included (`[text field, filled]`), and
  password fields are always masked (FR-012, US3).
- **`click(n)` / `type(n, text)`**: by snapshot number, with CDP input
  events. `type` refuses line breaks, so pressing Enter can't submit.
  A click on a submit control (`button` in a form without a `type`,
  `type=submit`, `input[type=submit|image]`) is gated (FR-007).
- **`read(url)`**: a fresh browser context (`Target.createBrowserContext`),
  navigate, then extract the main content (article/main or the densest
  text block) to Markdown, capped (default 50 KB, 20 s), then dispose
  of the context.
- **`login(site)`**: compares the page's current host with the
  credential's saved site; it must be the same host or a subdomain of
  it, compared label by label, so `example.org.evil.com` and
  `examp1e.org` fail.
  - It focuses the username, password and one-time-code fields and
    inserts the values with `Input.insertText`; the gateway computes
    TOTP codes itself (RFC 6238).
  - The agent gets `{"ok": true|false}`.
  - The first use per (agent owner, site) needs approval.

**Alternatives**:
- A TypeScript Playwright controller inside the container (the
  architecture's `sesh-browser`): credentials would pass through
  untrusted code, and the domain check would live there. Rejected; the
  check and the fill stay in the trusted gateway.
- Adopting Auto Browser, easi.run or Nod: each puts its own server, with
  its own API and credential handling, between the gateway and the
  browser. Not evaluated further for 010; recorded.

## R6. The live view: screencast and input through the gateway

**Decision**:
- **Pages**: the gateway serves `/live/<grant>` on the approvals listener
  (`:7002`, the address phones already reach over Tailscale). The
  secret is in the URL fragment, so it never reaches logs or referrers.
  - The small window is view-only.
  - Its "open" button calls `window.open` (popups are allowed) for the
    larger window, `/live/<grant>/control`, which has take over, hand
    back and close.
- **Frames**: the gateway uses CDP `Page.startScreencast` (JPEG, about 5
  frames a second, scaled) and forwards frames over a WebSocket to the
  page.
- **Input**: input from the larger window, only while the person is in
  control, is sent as CDP input events. The text the person types never
  enters any reply to the agent (FR-012).
- **Grants**: bound to the Open WebUI user from R2, the agent and the
  session. They expire 30 minutes after the last use; the secret is
  stored hashed; every grant, takeover and hand-back is audited.
- **Control**: in control, the agent's browser calls wait up to 60 s,
  then fail with "the person is in control". Closing the larger window,
  or its WebSocket dropping, hands back. A takeover lasts at most 15
  minutes.

## R7. Credentials for sites

**Decision**: a site login is stored through feature 004's credential
form, as the `browser` connector's credential `SITE_<host>` (username,
password and optional TOTP seed in one value). The operator runs
`sesh credential request browser SITE_example.org`. The gateway alone
reads it (constitution III). Approved first logins are recorded in the
gateway's database, keyed by owner and site.

## R8. Tests

- **The stub serves fake sites** under several host names: a login form,
  a page with a form, an article, a redirect to `10.0.0.1`, and a page
  that loads a subresource from `192.168.1.1`. The names are Compose
  network aliases of the stub, reached through the egress proxy, which
  in stub mode only (like Miniflux's private-address setting) allows
  the stub's own address. The check is still exercised: a separate
  test asserts it refuses the stub's address outside stub mode.
- **`browser_stack`**: every acceptance scenario, run through MCP like
  `gateway_stack`, plus the live view over its WebSocket, checked from
  the test.
- **`agent_stack`** gains one step: the stub script makes Hermes open
  the browser, and the Open WebUI message has an embed.

## R9. Downloads and uploads without the bytes reaching the model

**Downloads**:
- **Capture**: the gateway enables CDP's `Fetch` domain at the response
  stage. It recognises a download by `Content-Disposition: attachment`,
  or by a type the browser wouldn't render.
  - It reads the body (`Fetch.takeResponseBodyAsStream`, stopping at the
    size limit) into its own store.
  - It then fails the request in the browser, so nothing is saved
    inside the container.
- **Limits**: 25 MB per file; only PDF, PNG, JPEG, plain text, CSV and
  Office types; at most 20 kept per agent.
- **Storage**: kept in memory, then spilled to a gateway-owned
  directory for larger files, for 24 hours.
- **What the agent gets**: a reference, via `refs.rs`, which grows a
  source kind: `GmailAttachment` (fetched when used, as today),
  `Download` (bytes held) or `Document` (a Paperless document of the
  owner's).

**Why not a shared volume** (browser writes, gateway reads): it adds a
file channel between an untrusted container and the gateway, and leaves
copies in the browser's filesystem. The `Fetch` capture keeps one copy,
in the gateway.

**Uploads**:
- `browser.upload({n, ref})` needs the field to be a file input in the
  current snapshot.
- It is gated: the approval text shows the file name, size, type, the
  page's host and the field's label.
- After a yes, the gateway resolves the ref and fetches the bytes (from
  Gmail, Paperless or its download store). Its own page script builds a
  `File` from them and assigns it to the input with a `DataTransfer`,
  then fires `input` and `change`. Bytes are passed as a CDP call
  argument, capped at 25 MB.
- `DOM.setFileInputFiles` would need the file on the container's disk,
  so it isn't used.

**Takeover**:
- While the person is in control, downloads are captured the same way,
  and the control page lists them as kept for the agent.
- A file field clicked by the person opens a picker in the control
  page, on the person's own device. The chosen file is sent over the
  live view's WebSocket and set the same way. The person's own action
  needs no approval.

**Documents as a source**: `docs.ref({id})` issues a reference to one of
the owner's Paperless documents (scope checked as in `docs.get`). It
is a small addition to the `docs` connector, needed so stored documents
can be uploaded.

## R10. Implementation notes and deviations (2026-11-28)

- **Tested against a real Chromium.** `crates/sesh-gateway/tests/browser_chrome.rs`
  ran against Chromium 153 locally, through the egress proxy, covering
  `read`, the refused redirect and subresource, field masking, submit
  detection, download capture and refusal, upload, screencast frames and
  takeover. It is ignored unless `SESH_TEST_CHROME` is set; `browser_stack`
  covers the same in the containers.
- **Live grants and approved first logins are kept in the gateway's
  memory**, not in Postgres tables (data-model.md). A gateway restart ends
  open live views (the window says so) and asks again for a site's first
  login. Persisting them is a small follow-up if that proves annoying.
- **Downloads are held in memory** (at most 20 per agent, 25 MB each),
  not spilled to a directory; references last 24 hours.
- **Uploading from the person's own device during a takeover** (FR-020,
  a MAY) is not built: the person can click and type in the page, and
  downloads during a takeover are captured, but the control window has no
  file picker yet. Follow-up.
- **Site logins** use a dedicated form (`sesh browser login <owner>
  <site>`, username, password, optional authenticator key), stored under
  a credential name derived from a hash of owner and site, so the store's
  name rules (`[A-Z0-9_]{1,64}`) hold.
- **The chat listener doesn't check the agent's API key itself**: the
  runtime does (`API_SERVER_KEY`), and the live window is added only to a
  successful streamed reply with a valid signed user header.
- **Debian's apt mirrors aren't reachable from the development sandbox**,
  so the browser image was built only in CI; the image's flags were
  checked with the same Chromium locally.
- **CDP on loopback only.** New headless Chromium ignores
  `--remote-debugging-address` and listens on 127.0.0.1 (checked with
  Chromium 153 locally), so the image runs `socat` to expose port 9222 on
  the container's address, which is only on `browser-<agent>`.
