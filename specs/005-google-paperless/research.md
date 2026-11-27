# Research: Google and Paperless, with per-user identity

## R1. Owners and users

- **Agent records gain `owner`.** It's a user name, or `household`; the
  default is `household`, so feature 002–004 agents and tests keep
  working.
- **`sesh user add|remove|list`** keeps users in `gateway/users.toml`.
  Each user has a name (`[a-z][a-z0-9-]{0,31}`) and an approval topic.
  `household` always exists and can't be removed.
- **Scope comes from the owner, at call time.** The gateway works out
  everything a call may touch from the agent's owner: the Google grant
  key, the Paperless owner id, and the approval topic. The model never
  names any of these.
- **Removing a user** deletes their Google grant (with a revoke) and
  their consent links. Their agents' records stay, but every call is
  refused with "owner removed" until the operator removes those agents.
- **Rationale:** constitution IV. The model never supplies a scope; the
  token's agent → owner chain decides it.

## R2. Approval channels per user

- **Topics:** each user gets an ntfy topic, `sesh-<name>-<random>`. The
  Household's is the existing `ntfy_topic`, so feature 004 set-ups keep
  their topic.
- **ntfy users:**
  - one per sesh user, with read access to their own topic and the
    Household's;
  - `sesh-gateway`, with write access to every topic.

  Passwords are generated into `secrets.env` as `NTFY_PASSWORD_<NAME>`.
  The existing `person` user becomes the Household's reader, and every
  user can read the Household topic.
- **Routing:** a gated call from Ana's agent goes to Ana's topic. A
  Household agent's goes to the Household topic, which every user
  reads, and the first decision settles it (FR-004).

## R3. Google OAuth (constitution III)

- **The OAuth client:** the household's own OAuth client, a "Web
  application" type in their Google Cloud project. Its client id and
  secret are entered through one-time credential forms (feature 004) as
  connector `google`, names `CLIENT_ID` and `CLIENT_SECRET`.
- **The consent flow:**
  1. `sesh google connect <user>` calls `/admin/google/consent` and
     prints `<base>/google/start/<id>#<secret>`. The link works once and
     expires after 30 minutes.
  2. The start page posts the secret back. The gateway checks it, marks
     the link used, and creates an OAuth `state` (random, single use,
     bound to the user, 10 minutes) and a PKCE verifier.
  3. It redirects to Google's authorization endpoint, with:
     - `access_type=offline`, `prompt=consent`;
     - scopes `gmail.readonly gmail.compose calendar.events contacts.readonly`;
     - `redirect_uri=<base>/google/callback`.
  4. `/google/callback?code&state` checks the state, then exchanges the
     code with the PKCE verifier at the token endpoint.
  5. The refresh token is stored in Infisical as
     `USER_<NAME>__GOOGLE__REFRESH_TOKEN`. The granted scopes go in the
     gateway's Postgres table `google_connections`, as metadata with no
     secret.
- **Per call:** the gateway exchanges the refresh token for an access
  token, caches it in memory until shortly before expiry, and never logs
  it. If Google answers `invalid_grant`, the connection is marked
  revoked, and calls answer "Google access was revoked; reconnect".
- **Disconnecting:** `sesh google disconnect <user>` calls Google's
  revoke endpoint, deletes the Infisical secret, and marks the
  connection removed.
- **Configurable addresses:** the authorization, token, revoke, Gmail,
  Calendar and People base URLs are all gateway settings. Tests point
  them at the stub's fake Google.
- **Callback address:** Google requires `https` or `localhost` for
  redirect URIs. The base is `approvals_base_url`: `http://127.0.0.1:7002`
  for a laptop browser, or the Tailscale HTTPS URL for a phone. The
  operator registers that callback in the Google client.
- **Alternatives:**
  - *Device flow:* it doesn't support Gmail scopes for most clients.
  - *A Google MCP server* (the architecture's original idea): it would
    hold the tokens itself, outside the gateway, against III. Calling the
    APIs directly from the gateway keeps one trusted path. This needs an
    ADR (0008).

## R4. Connector names

The architecture's `google.gmail.search` becomes three connectors, so
that grants and scripts read naturally (`mail.send(...)`):

| Connector | Methods | Gated |
|---|---|---|
| `mail` | `search({query, limit?})`, `read({id})`, `draft({to, subject, body, cc?})`, `send({to, subject, body, cc?})` | `send` |
| `calendar` | `list({from?, to?, limit?})`, `create({title, start, end, guests?, location?})` | `create` |
| `contacts` | `search({query, limit?})` | none |
| `docs` | `search({query, limit?})`, `get({id})`, `upload({ref, title, tags?})` | none (Household uploads notify) |

All three Google connectors use the owner's one Google grant. Approval
text comes from the existing gateway template (feature 004), with the
values relevant to the method (FR-009).

## R5. Attachment references

- **Issuing:** `mail.read` returns attachments as
  `{ref, filename, mime, size}`. A `ref` is 16 random bytes in hex,
  mapped in gateway memory to (agent, message id, attachment id,
  filename, mime, size, expiry in 1 hour).
- **Using:** a ref works only for the agent it was issued to. It is
  lost on a gateway restart, which is acceptable at a 1-hour lifetime.
- **Filing:** `docs.upload({ref, ...})` fetches the bytes from Gmail
  (`attachments.get`) inside the gateway, checks the type and size
  limits, and uploads to Paperless. The bytes never leave the trusted
  plane.

## R6. Paperless-ngx

- **The service:** `paperlessngx/paperless-ngx:3.5.1`, pinned by digest,
  using:
  - database `paperless` in the existing Postgres, as the `sesh` owner;
  - Redis database 2;
  - an admin user and password generated into `secrets.env`;
  - OCR language English;
  - no consumption folder.
- **Network:** an internal network, `docs`, shared only with the
  gateway. Paperless has no internet need.
- **The one module that reaches Paperless** is `connectors/docs.rs` in
  the gateway. The CLI never calls Paperless.
  - It gets a token as the admin through `/api/token/`, and caches it.
  - It finds or creates one Paperless user per sesh user, named
    `sesh-<name>`, the first time it's needed.
- **Scoping:**
  - Searches use `/api/documents/?query=…&owner__id__in=<ids>`, where the
    ids are the owner's user, plus the Household's for personal agents.
  - `get` checks the document's `owner` against the same set, so another
    user's document id answers "not found".
- **Uploads:**
  1. `post_document` returns a task id.
  2. The gateway polls `/api/tasks/?task_id=` until the task finishes
     (60 s limit), to get the new document's id.
  3. It sets the owner with `PATCH /api/documents/<id>/` (`owner` = the
     owner's Paperless id, no extra permissions).
  4. A Household upload notifies the Household topic.
- **Rationale:** the gateway enforces scope (I and IV). The admin token
  never leaves the gateway, and Paperless's own owner model keeps
  documents apart for people using its web UI too.

## R7. Fakes for tests and the eval

- **Fake Google, in the stub:**
  - authorization (it redirects straight back with a code);
  - token (refresh and exchange);
  - revoke;
  - Gmail messages list, get and attachments, drafts create and messages
    send. Sends are recorded and countable at `GET /fake/google/sent`.
  - Calendar events list and insert;
  - People `searchContacts`.

  Per-user mailboxes are keyed by the refresh token the stub issued, so
  cross-user tests are real.
- **Fake Paperless, in the stub, for the eval only:** search, get,
  `post_document`, tasks and patch. The stack tests use the real
  Paperless image.

## R8. Eval suite v3

New tasks:
- `mail-fact`: find a fact in a message, and cite its subject.
- `mail-draft`: a draft is created and nothing is sent.
- `mail-send`: approve, then act, with exactly one send.
- `file-attachment`: file by ref. It fails if the answer contains the
  attachment's text, the "pasted contents" rule.

The eval gateway gets a test user, a fake Google grant from the stub's
consent flow, and the fake Paperless.

## R9. Boundary change → ADR 0008

- **New connections:** the gateway now calls Google's APIs and Paperless
  directly; there is no Google MCP server.
- **New gateway state:** user records with owners, and consent links.
- **The diagram:** it already shows Google and Paperless behind the
  gateway, so its components don't change. The architecture document's
  catalog row for Google ("Google APIs via MCP server") changes to
  "Google APIs, called by the gateway".

## Build decisions (where the build differs from the plan above)

- **No new tables.** A Google connection's scopes and state live next to
  its grant in the secret store (`USER_<NAME>__GOOGLE__SCOPES`,
  `__STATE`), and consent links live in gateway memory; T002's tables
  were not needed. A restart forgets open consent links, which only
  means asking for a new one.
- **Approval text** uses the gateway's generic template (the agent, the
  method, and the quoted arguments, so recipients and subject show).
- **Uploads within the deadline.** An execution has 30 s. `docs.upload`
  waits up to 20 s for Paperless to consume the file; past that it
  answers `processing` and a background task gives the document its
  owner. Until then the document belongs to the gateway's admin
  account, which no agent's scope includes: invisible, never misfiled.
  The Household notification is sent when Paperless accepts the file.
- **Upload types:** PDF and images only; Office files need Tika and
  Gotenberg, which sesh doesn't run.
- **Paperless networking:** `paperless` is on the internal `docs`
  network with the gateway, and on `default` for Postgres and Redis.
- **Doctor `paperless` warns** rather than fails: Paperless takes a
  minute or two to start, and its absence isn't a safety problem.
- **The Google OAuth client** (`GOOGLE__CLIENT_ID`, `__CLIENT_SECRET`) is
  filled through the feature 004 one-time form, as a `google`
  pseudo-connector; `sesh credential list` shows it once an agent uses a
  Google connector.
- **The eval gateway** gets an in-process secret store
  (`SESH_SECRET_STORE=memory`, never used by the household stack) and a
  per-run admin token. `sesh eval` fills the fake client credentials
  through the real form and connects the Household through the real
  consent flow with fake Google's code. Suite v3 adds two scoring
  rules: `forbidden` calls, and `pasted` file text.
- **The fake consent:** tests read the `state` from the authorization
  URL and call the callback with `code=fake-<user>` themselves (the stub
  has no host port, so a browser redirect isn't simulated).

## CI results (T022)

Run 46025194506 (commit 652277f) passed every job: `check`, `images`,
and the stack tests `foundation_stack`, `gateway_stack`, `eval_stack`
(suite v3, 10 of 10 tasks with the pass script, each new task failing for
its own reason with the fail script), `secrets_stack` and
`google_docs_stack` (real Paperless image, fake Google, no internet).

Three runs failed first, all at `google_docs_stack`'s document search,
and each taught something about the real Paperless:
- **45945393488:** the search found nothing. The failure didn't print the
  search, so the test now prints the upload result and the last search.
- **45948479363:** the upload stayed `processing` though Paperless
  finished in about 6 s: the gateway read only `related_document` and the
  list's first entry. It now picks its own task by `task_id` and reads
  the id from `related_document` or the `result`, in any letter case.
- **45950847250:** Paperless marked the task `SUCCESS` without naming the
  document in any shape the gateway read. The gateway now waits a
  moment, then finds the newest document still owned by its own account
  with the upload's exact title and file name, never another user's, and
  logs the task entry it saw.

Lesson: Paperless's task API differs between versions; the gateway must
not depend on one shape of it.
