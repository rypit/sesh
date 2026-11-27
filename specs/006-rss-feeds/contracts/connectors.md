# Contract: the `feeds` connector

Arguments follow feature 002's schema subset; unknown properties are
refused. No method takes a user, owner or account. Every result is
untrusted content.

| Method | Input | Result |
|---|---|---|
| `feeds.list` | `{}` | `[{name, title, url, owner: "you" \| "household"}]`; `title` is null until the feed has been fetched |
| `feeds.latest` | `{feed: name 1..32, since?: date or date-time, limit?: 1..25}` | `{feed, title, items: [{title, link, published, summary}], more: bool}` |
| `feeds.add` (gated) | `{url: string 1..2000, name?: name 1..32}` | `{added, url}` |

- `feed` names one of the agent's readable feeds: its owner's first,
  then the Household's.
- `since` keeps items published at or after it; items without a
  readable date are skipped when it is given.
- `limit` defaults to 10.

**Errors, in the result's `error`:**
- "no feed named <name>; see feeds.list";
- "only http and https feed addresses are allowed";
- "that address isn't on the public internet";
- "the feed redirected to an address that isn't public";
- "the feed redirected too many times";
- "the feed is too large";
- "the feed took too long";
- "the feed answered <status>";
- "the feed's host couldn't be found";
- "this isn't an RSS or Atom feed";
- "a feed named <name> already exists";
- "this owner already has <n> feeds".

**Approval text** (`feeds.add`): the gateway template, with `url` and
`name` quoted; the description reads "Follow a new feed: from now on the
gateway fetches this address for your owner's agents."
