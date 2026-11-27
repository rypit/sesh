# Contract: the `feeds` connector

Arguments follow feature 002's schema subset; unknown properties are
refused. No method takes a user, owner or account. Every result is
untrusted content. The connector is the gateway's only route to
Miniflux.

| Method | Input | Result |
|---|---|---|
| `feeds.list` | `{}` | `[{name, url, owner: "you" \| "household"}]` |
| `feeds.latest` | `{feed: name 1..32, since?: date or date-time, limit?: 1..25}` | `{feed, owner, items: [{title, link, published, summary}], more: bool}` |
| `feeds.add` (gated) | `{url: string 1..2000, name?: name 1..32}` | `{added, url}` |

- `feed` names one of the agent's readable feeds: its owner's first,
  then the Household's.
- `since` keeps items published at or after it.
- `limit` defaults to 10.

**Errors, in the result's `error`:**
- "no feed named <name>; see feeds.list";
- "only http and https feed addresses are allowed";
- "a feed address can't carry a user name or password";
- "a feed named <name> already exists";
- "this owner already has <n> feeds";
- "the feed service couldn't add that feed: <reason>";
- "the feed service is unavailable".

**Approval text** (`feeds.add`): the gateway template, with `url` and
`name` quoted; the description reads "Follow a new feed: from now on
the feed service fetches this address for your owner's agents."
