# Feature Specification: RSS feeds

**Feature Branch**: `claude/rss-feeds`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Feature 006 — RSS feeds. Feeds belong to an owner (a household user, or the Household). A personal agent reads its owner's feeds and the Household's; the Household agent reads only the Household's. People add, remove and list feeds with the admin CLI. Agents get a `feeds` connector: list the feeds they can read, read a feed's latest items, and ask to add a feed, which waits for the owner's approval because a feed address chosen by an agent is fetched on its behalf. RSS is not a special case: it follows the same resource flow as search and documents — an adopted feed service behind the gateway, reached through exactly one module, used by agents through code mode. The gateway itself never fetches arbitrary addresses. Feed text is untrusted data. Every call and change is audited. Doctor reports feeds. Tests use a fake feed publisher, never the internet; the eval gains feed tasks. Read-only Google Docs moves to feature 007 and memory follows. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **The operator** runs sesh.
- **Household users** each have their own feeds.
- **The Household** has shared feeds every user's agent can read.
- **Agents** act for one owner (feature 005). In this phase the test
  client and the eval play them.

### User Story 1 - People choose the feeds (Priority: P1)

The operator adds a news feed for Ana, and one for the whole Household,
by its address and a short name. They can list and remove feeds. sesh
refuses an address that isn't an http or https web address, or that the
feed service can't read as a feed.

**Why this priority**: Without feeds there is nothing to read. Feeds are
operator configuration (constitution II), so people choose them first.

**Independent Test**: Add a Household feed and one for Ana, list them,
remove one. Adding `ftp://…` or an address that isn't a feed is refused
with a reason, and nothing is added.

**Acceptance Scenarios**:

1. **Given** user Ana, **When** the operator adds a feed for Ana with a
   name, **Then** it is listed under Ana, and the change is audited.
2. **Given** an address that isn't http or https, carries a password, or
   isn't a feed, **When** the operator adds it, **Then** it is refused
   with the reason, and nothing changes.
3. **Given** a feed, **When** the operator removes it, **Then** no agent
   can read it any more, and the removal is audited.
4. **Given** an unknown user, **When** the operator adds a feed for
   them, **Then** it is refused.

---

### User Story 2 - Agents read their owner's feeds (Priority: P1)

Ana's agent lists the feeds it can read (Ana's and the Household's) and
reads the latest items of one: headline, link, date and a short summary,
as plain text. Ben's agent sees Ben's and the Household's feeds, never
Ana's. The Household agent sees only the Household's.

**Why this priority**: This is the feature's value: "what's new on the
harbour news?" from either person's agent.

**Independent Test**: With a fake feed publisher, Ana's agent reads
Ana's feed and the Household's; Ben's agent can't list or read Ana's
feed, even by naming it; an extra argument naming a user is refused.

**Acceptance Scenarios**:

1. **Given** feeds for Ana, Ben and the Household, **When** Ana's agent
   lists feeds, **Then** it sees Ana's and the Household's only.
2. **Given** Ana's feed, **When** Ben's agent asks for its items by name,
   **Then** it is told there's no such feed.
3. **Given** a feed, **When** an agent reads it, **Then** each item has a
   title, link, date and summary as plain text, each cut to a limit, and
   the whole result is capped.
4. **Given** a feed item whose text tells the agent to do something,
   **When** the agent reads it, **Then** it arrives as data, and nothing
   runs because of it.

---

### User Story 3 - An agent asks to add a feed (Priority: P2)

Ana asks her agent to follow a new feed. The agent asks to add it; Ana
gets an approval request showing exactly the address and name. Only
after she approves is it added to Ana's feeds. An address that isn't a
web address is refused at once, without bothering Ana.

**Why this priority**: Useful, but an address chosen by an agent and
fetched on its behalf could carry data out to anyone's server, so it
must be a person's decision (constitution V, VI).

**Independent Test**: Ana's agent asks to add a feed: it waits for
approval on Ana's topic; after approval, a re-run adds it and it is
listed; Ben's agent can't see it. Asking to add `file:///etc/passwd` is
refused before any approval is asked.

**Acceptance Scenarios**:

1. **Given** Ana's agent, **When** it asks to add a feed, **Then**
   nothing is added, Ana is asked with the address and name, and the
   agent is told approval is pending.
2. **Given** Ana approves, **When** the agent re-runs the same request,
   **Then** the feed is added to Ana's feeds, once.
3. **Given** a non-web address, a duplicate name, or an owner at the
   limit, **When** the agent asks to add it, **Then** it is refused at
   once, and no one is asked.
4. **Given** the Household agent, **When** it asks to add a feed,
   **Then** the request goes to every user, and any one may decide.

---

### User Story 4 - The feed service can't reach inside (Priority: P1)

The feed service is the only part of sesh that fetches feeds. It reaches
the internet and its own database, nothing else: not the gateway's
networks with sesh's other services, not the home network through
sesh. Agents and their executors can't reach the feed service at all;
only the gateway can.

**Why this priority**: Constitution I and II. A feed address is chosen
by a person or an agent, so whatever fetches it must sit where a bad
address can do no harm.

**Independent Test**: In the running stack, the feed service isn't on
the network the gateway shares with Postgres, ntfy and the rest;
probing the gateway and Postgres there fails; an executor can't resolve
or reach the feed service.

**Acceptance Scenarios**:

1. **Given** the running stack, **When** the feed service tries the
   gateway or Postgres on the shared network, **Then** it can't reach
   them.
2. **Given** an executor, **When** it tries the feed service, **Then**
   the name doesn't resolve and nothing connects.
3. **Given** a feed that fails to update, **When** the operator runs
   doctor, **Then** it warns, naming the feed.

---

### User Story 5 - Evals cover feeds (Priority: P3)

The eval suite gains tasks that use feeds, against fakes:
- report the newest headline from a named feed, and cite its link;
- add a feed, only with approval.

**Why this priority**: The operator should know whether a model can use
feeds. It follows the feature itself.

**Independent Test**: The stub's passing script passes the new tasks;
its failing script fails them for the expected reasons.

**Acceptance Scenarios**:

1. **Given** the eval, **When** it runs, **Then** feeds come from fakes,
   never the internet.
2. **Given** a model that claims to have added a feed without re-running
   after approval, **When** scored, **Then** the task fails with that
   reason.

### Edge Cases

- **A feed name used by both Ana and the Household:** Ana's agent reads
  Ana's; the Household's is listed with its owner.
- **A user removed with feeds:** their feeds are kept but unreachable,
  since their agents stop working (feature 005); the operator removes
  them with the CLI.
- **A publisher that is down:** the feed service keeps the items it has
  and records the error; doctor warns.
- **Very long items or hundreds of items:** each field is cut, items are
  capped, and the result says there are more.
- **An agent asking to add a feed that already exists, or past the
  per-owner limit:** refused before any approval is asked.
- **The feed service is down:** feed methods answer "unavailable"; other
  connectors still work.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The operator MUST be able to add, list and remove feeds
  for any user or the Household, by address and name.
- **FR-002**: Feeds MUST belong to exactly one owner. A personal agent
  MAY read its owner's and the Household's feeds; the Household agent
  reads only the Household's. Scope MUST come from the agent's owner,
  never from an argument, and the gateway MUST enforce it itself,
  whatever the feed service returns.
- **FR-003**: Feed addresses MUST be http or https, with a host, no
  embedded credentials, and at most 2,000 characters.
- **FR-004**: Only the adopted feed service fetches feeds; the gateway
  MUST NOT fetch a feed address itself.
- **FR-005**: The feed service MUST reach only the internet and its own
  database: no network it shares with sesh's other services except the
  one it shares with the gateway alone. Agents and executors MUST NOT be
  able to reach it.
- **FR-006**: Every feed's items MUST come from the owner's own account
  in the feed service; one account per owner, created by the gateway.
- **FR-007**: Each item MUST be returned as title, link, date and
  summary in plain text, with markup removed, each field cut to a
  limit, items capped per call, and the whole result capped.
- **FR-008**: Feed content MUST be treated as untrusted data: never as
  instructions or configuration, and never able to add feeds or change
  what any agent can do.
- **FR-009**: An agent's request to add a feed MUST need the owner's
  approval (the Household's goes to every user), showing the exact
  address and name. Invalid, duplicate or over-limit requests MUST be
  refused before anyone is asked.
- **FR-010**: No feed-service credential MUST appear in any result,
  log, audit row or data-directory file other than `secrets.env`.
- **FR-011**: Every feeds call, every operator change, and every
  approved add MUST be audited, with the owner.
- **FR-012**: Doctor MUST report whether the gateway can reach the feed
  service, how many feeds each owner has, and warn when a feed's last
  update failed.
- **FR-013**: Tests MUST use a fake feed publisher (and, for the eval, a
  fake feed service), never the internet.
- **FR-014**: Negative tests MUST cover: another owner's feed; an
  argument naming a user; an add without approval; the feed service
  reaching the shared network; an executor reaching the feed service.
- **FR-015**: The eval suite MUST gain the tasks in User Story 5.

### Key Entities

- **Feed:** an owner, a short name (unique per owner), an address; held
  in the owner's feed-service account.
- **Feed account:** one per owner in the feed service, created and used
  only by the gateway.
- **Feed item:** title, link, date and summary, as plain text.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of cross-owner reads in tests are refused.
- **SC-002**: In the running stack, the feed service reaches neither the
  gateway nor Postgres on the shared network, and no executor reaches
  the feed service.
- **SC-003**: An agent-requested feed is added only after approval, and
  exactly once, in 100% of tests.
- **SC-004**: No feed-service credential appears in any agent result,
  log or audit row.
- **SC-005**: A feed added by the operator is readable by its owner's
  agent within 1 minute.
- **SC-006**: The full test suite passes in CI with no internet.

## Assumptions

- **The feed service:** Miniflux (Apache-2.0), self-hosted, with its own
  database in the existing Postgres. One Miniflux user per owner
  (`sesh-<owner>`); the gateway signs in as Miniflux's admin to manage
  them, and as each owner's user to read and add feeds. Miniflux fetches
  and parses feeds and keeps their items; it polls on its own schedule.
- **Where feeds live:** in Miniflux only. The gateway keeps no feed
  table; the owner mapping is the user name, as for Paperless.
- **Names:** `[a-z0-9][a-z0-9-]{0,31}`, stored as the feed's title in
  Miniflux; without `--name`, the first label of the host (`www.`
  dropped).
- **Limits:** 50 feeds per owner; 25 items per read (10 by default);
  titles 300 characters, summaries 1,000, links 2,000; results 20 kB.
- **Networks:** an internal `feeds` network shared only by the gateway
  and Miniflux; `feeds-egress` for Miniflux to reach publishers; an
  internal `feeds-db` network shared only by Miniflux and Postgres.
- **No discovery:** the address must be the feed itself.
- **Out of scope:** OPML import; full-article fetching (the browser's
  `read(url)`, feature 010); per-item read state; Miniflux's own web
  UI for people (not published).
- **Roadmap:** this is feature 006; read-only Google Docs becomes 007,
  and memory follows (008).
