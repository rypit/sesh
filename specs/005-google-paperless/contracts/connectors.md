# Contract: mail, calendar, contacts and docs connectors

All arguments follow feature 002's schema subset, and unknown properties
are refused. Every result is untrusted content. No method takes a user,
account or owner argument.

| Method | Input | Result |
|---|---|---|
| `mail.search` | `{query: string 1..300, limit?: 1..25}` | `[{id, from, to, subject, date, snippet}]`, with `more: bool` |
| `mail.read` | `{id: string}` | `{id, from, to, cc, subject, date, body, attachments: [{ref, filename, mime, size}]}`; `body` is plain text, cut at 20 kB |
| `mail.draft` | `{to: [email 1..20], subject: 1..300, body: 1..50000, cc?: [email ..20]}` | `{draft_id}` |
| `mail.send` (gated) | same as `draft` | `{message_id}` |
| `calendar.list` | `{from?: date-time, to?: date-time, limit?: 1..50}` | `[{id, title, start, end, location, guests}]` |
| `calendar.create` (gated) | `{title, start, end, guests?: [email ..50], location?}` | `{event_id}` |
| `contacts.search` | `{query, limit?: 1..25}` | `[{name, emails}]` |
| `docs.search` | `{query, limit?: 1..25}` | `[{id, title, created, tags, owner: "you" \| "household"}]` |
| `docs.get` | `{id: integer}` | `{id, title, tags, created, content}`; `content` is Paperless's text, cut at 20 kB |
| `docs.upload` | `{ref: string, title: 1..200, tags?: [string ..10]}` | `{id, title}` |

**Errors, in the result's `error`:**
- "Google isn't connected for this user";
- "Google access was revoked; reconnect";
- "not permitted by this Google connection" (a missing scope);
- "unknown or expired attachment reference";
- "file type not allowed";
- "file too large";
- "document not found".

**Approval text:**
- `mail.send`: "Send an email to <to> (cc <cc>), subject "<subject>": <first 300 characters of the body>".
- `calendar.create`: "Create "<title>" from <start> to <end>, guests <guests>".
