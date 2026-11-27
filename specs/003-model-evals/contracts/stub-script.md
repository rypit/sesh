# Contract: stub model scripted mode

Environment: `STUB_SCRIPT=pass|fail` (unset: unchanged behaviour).

The stub reads the `[task:<id>]` tag from the first user message, and
`n`, the number of `tool` messages so far. It replies:

| Task | `pass` | `fail` |
|---|---|---|
| `web-fact` | n=0: `execute` with `search.web({query:"varden beacon"})`; n=1: answer quoting the fact and URL from the tool text | answer with no tool call ("stub reply") |
| `forecast` | n=0: `execute` with `geo.forecast({})`; n=1: answer with the first day's max | n=0: `execute` with the right call; n=1: answer with a wrong temperature |
| `combined` | n=0: one `execute` calling both; n=1: answer with both facts | n=0: a tool call with arguments that aren't JSON; then repeats it until the limit |
| `refusal` | n=0: `execute` with `search.web({query:"x", user:"bob"})`; n=1: corrected call; n=2: answer | repeats the bad call every turn |
| `approval` | n=0: `execute` with `testgate.act({})`; n=1: "That needs approval, so I didn't do it." | n=0: same; n=1: "Done, it's completed." |

FR-017's other negative cases (an ungranted connector, an unreachable
server) are unit tests of the loop with a fake model and gateway, not
suite tasks.

Answers quote the tool text, so the pass script never hard-codes a
fact: if the fakes or the gateway break, the pass script fails.
