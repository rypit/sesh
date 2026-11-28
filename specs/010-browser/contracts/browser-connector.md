# Contract: the `browser` connector

Code mode methods (the agent calls them inside `execute`):

| Method | Arguments | Returns | Gated |
|---|---|---|---|
| `browser.read` | `{url}` | `{title, url, markdown, truncated}` (untrusted) | no |
| `browser.open` | `{url}` | a snapshot | no |
| `browser.snapshot` | `{}` | `{url, title, text, elements: [{n, kind, label, filled?}]}` | no |
| `browser.click` | `{n}` | a snapshot | yes, when `n` submits a form |
| `browser.type` | `{n, text}` (no line breaks) | a snapshot | no |
| `browser.login` | `{site}` | `{ok}` | the first time per owner and site |
| `browser.downloads` | `{}` | `[{ref, filename, type, size, from}]` (this session's) | no |
| `browser.upload` | `{n, ref}` | a snapshot | yes, always (shows file name, size, site) |
| `docs.ref` | `{id}` | `{ref, filename, type, size}` | no |

A click that triggers a download returns the snapshot plus
`downloads: [{ref, filename, type, size}]`. References work with
`docs.upload` and `browser.upload`, only for the agent they were issued
to, for 24 hours.

Errors (as text, like other connectors):
- `that address is on the home network; the browser can't open it`
- `the browser isn't on <site> (it's on <host>); not logging in`
- `no login is stored for <site>; ask the person to add one with sesh credential request browser SITE_<site>`
- `the person is in control of the browser; try again later`
- `only http and https addresses can be opened`
- `that download is too large (limit 25 MB)` / `that kind of file isn't allowed`
- `element <n> isn't a file field`
- `that file reference isn't yours or has expired`

# Contract: the live view (approvals listener, `:7002`)

| Path | What |
|---|---|
| `GET /live/<grant>` | small, view-only page; the secret is in `#fragment` |
| `GET /live/<grant>/control` | larger page with take over / hand back / close |
| `GET /live/<grant>/ws` | WebSocket: the first message is the secret; frames from the server; input and control messages from the control page |

# Contract: the chat listener (`:7003`)

`POST /v1/chat/completions` and `GET /v1/models`, OpenAI-compatible:
- authenticated with the agent's API key;
- the Open WebUI user is taken from the `X-OpenWebUI-User-Jwt` header;
- requests are forwarded to the agent's runtime, and the reply streams
  back unchanged, plus at most one `{"event": {"type": "embeds", …}}`
  chunk per request.
