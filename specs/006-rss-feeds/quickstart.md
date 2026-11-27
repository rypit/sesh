# Quickstart: RSS feeds

```bash
sesh init --stub && sesh up
sesh user add ana
sesh agent add ana-agent --connectors feeds --owner ana
sesh feed add household http://stub-llm:8000/fake/feeds/harbour.xml --name harbour
sesh feed add ana http://stub-llm:8000/fake/feeds/garden.xml --name garden
sesh feed list
```

Then, as `ana-agent`:
- `feeds.list({})` shows `garden` (yours) and `harbour` (household).
- `feeds.latest({feed: "harbour", limit: 3})` returns the newest items
  as plain text.
- `feeds.add({url: "http://stub-llm:8000/fake/feeds/ferry.xml", name:
  "ferry"})` waits for Ana's approval; run it again once she approves.

With a real model, add real feeds the same way, e.g. `sesh feed add
household https://example.org/news.rss --name news`. Addresses on
the home network, the tailnet or the server itself are refused.

`sesh doctor` shows how many feeds each owner has, and warns about
feeds that failed on their last fetch.
