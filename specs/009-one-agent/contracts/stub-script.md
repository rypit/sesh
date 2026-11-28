# Contract: the stub's agent script

Environment: `STUB_SCRIPT=agent`. The eval tags (`[task:<id>]`) behave
as with `pass`.

The stub reads the `[agent:<step>]` tag from the **latest** user
message (Open WebUI sends the whole chat), and whether a `tool` message
follows it. The tool it calls is the offered tool whose name ends in
`execute`.

| Step | Before a tool result | After it |
|---|---|---|
| `weather` | `execute` with `geo.forecast({})` | "Forecast: " + the tool text |
| `search` | `execute` with `search.web({query:"varden beacon"})` | "Found: " + the tool text |
| `mail` | `execute` with `mail.search({query:"power bill"})` then `mail.read` of the first | "Mail: " + the tool text |
| `feeds` | `execute` with `feeds.latest({feed:"harbour",limit:3})` | "Feeds: " + the tool text |
| `send` | `execute` with the eval's `mail.send` (to `bills@example.org`) | "Send: " + the tool text |
| `terminal` | a call to `terminal` with `{"command":"id"}` | "Terminal: " + the tool text |

`GET /stub/requests` returns the last 50 chat requests' `model` and
offered tool names (never message content), newest last.
