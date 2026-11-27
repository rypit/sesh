# Feature Specification: RSS feeds

**Feature Branch**: `claude/rss-feeds`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Feature 006 — RSS feeds. Feeds belong to an owner (a household user, or the Household). A personal agent reads its owner's feeds and the Household's; the Household agent reads only the Household's. People add, remove and list feeds with the admin CLI. Agents get a `feeds` connector: list the feeds they can read, read a feed's latest items, and ask to add a feed, which waits for the owner's approval because a URL the gateway fetches for an agent is an exfiltration channel. Only the gateway fetches feeds, through one module, with http/https only, no private or internal addresses (checked again on every redirect), size, time and redirect limits and a per-feed cache. Feed text is untrusted data. Every call and change is audited. Doctor reports feeds. Tests use a fake feed server, never the internet; the eval gains feed tasks. Read-only Google Docs moves to feature 007 and memory follows. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

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
refuses an address that isn't a public web address: a file path, an
address on the home network, or the server itself.

**Why this priority**: Without feeds there is nothing to read. Feeds are
operator configuration (constitution II), so people choose them first.

**Independent Test**: Add a Household feed and one for Ana, list them,
remove one. Adding `ftp://…`, `http://127.0.0.1/…` or
`http://192.168.1.10/…` is refused with a reason, and nothing is added.

**Acceptance Scenarios**:

1. **Given** user Ana, **When** the operator adds a feed for Ana with a
   name, **Then** it is listed under Ana, and the change is audited.
2. **Given** an address that isn't http or https, or that points at a
   private, loopback or otherwise internal address, **When** the operator
   adds it, **Then** it is refused with the reason, and nothing changes.
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

**Independent Test**: With a fake feed server, Ana's agent reads Ana's
feed and the Household's; Ben's agent can't list or read Ana's feed,
even by naming it; an extra argument naming a user is refused.

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
5. **Given** an agent reading the same feed again within a few minutes,
   **When** it reads, **Then** sesh answers from its copy and doesn't
   fetch the publisher again.

---

### User Story 3 - An agent asks to add a feed (Priority: P2)

Ana asks her agent to follow a new feed. The agent asks to add it; Ana
gets an approval request showing exactly the address and name. Only
after she approves is it added to Ana's feeds. An address that isn't a
public web address is refused at once, without bothering Ana.

**Why this priority**: Useful, but a URL chosen by an agent and fetched
by the gateway could carry data out to anyone's server, so it must be a
person's decision (constitution V, VI).

**Independent Test**: Ana's agent asks to add a feed: it waits for
approval on Ana's topic; after approval, a re-run adds it and it is
listed; Ben's agent can't see it. Asking to add `http://169.254.169.254/`
is refused before any approval is asked.

**Acceptance Scenarios**:

1. **Given** Ana's agent, **When** it asks to add a feed, **Then**
   nothing is added, Ana is asked with the address and name, and the
   agent is told approval is pending.
2. **Given** Ana approves, **When** the agent re-runs the same request,
   **Then** the feed is added to Ana's feeds, once.
3. **Given** an internal or non-web address, **When** the agent asks to
   add it, **Then** it is refused at once, and no one is asked.
4. **Given** the Household agent, **When** it asks to add a feed,
   **Then** the request goes to every user, and any one may decide.

---

### User Story 4 - Feeds that misbehave can't reach inside (Priority: P1)

A feed that redirects to an internal address, answers too slowly, sends
too much, or sends something that isn't a feed is refused, with a
reason that names no internal address. sesh never reaches a private
address because of a feed.

**Why this priority**: Constitution I and II: the gateway sits on the
network with sesh's own services, so a fetch it makes for an agent must
never reach them.

**Independent Test**: Against the fake feed server, a feed that
redirects to a loopback or private address, and one larger than the
limit, are each refused; the results hold no internal address.

**Acceptance Scenarios**:

1. **Given** a feed that redirects to a private address, **When** an
   agent reads it, **Then** it is refused, and nothing is fetched there.
2. **Given** a feed larger than the limit, or slower than the time
   limit, **When** an agent reads it, **Then** it is refused.
3. **Given** a reply that isn't RSS or Atom, **When** an agent reads it,
   **Then** it is told the feed couldn't be read.
4. **Given** a feed that failed recently, **When** the operator runs
   doctor, **Then** it warns, naming the feed.

---

### User Story 5 - Evals cover feeds (Priority: P3)

The eval suite gains tasks that use feeds, against the fake feed server:
- report the newest headline from a named feed, and cite its link;
- add a feed, only with approval.

**Why this priority**: The operator should know whether a model can use
feeds. It follows the feature itself.

**Independent Test**: The stub's passing script passes the new tasks;
its failing script fails them for the expected reasons.

**Acceptance Scenarios**:

1. **Given** the eval, **When** it runs, **Then** feeds come from the
   fake feed server, never the internet.
2. **Given** a model that claims to have added a feed without re-running
   after approval, **When** scored, **Then** the task fails with that
   reason.

### Edge Cases

- **A feed name used by both Ana and the Household:** Ana's agent reads
  Ana's; the Household's is listed with its owner.
- **A user removed with feeds:** their feeds are kept but unreachable,
  since their agents stop working (feature 005); the operator removes
  them with the CLI.
- **A publisher that is down:** the read answers "unavailable", sesh
  remembers the failure briefly instead of retrying on every call, and
  doctor warns.
- **A host name that resolves to both public and private addresses:**
  refused.
- **Very long items or hundreds of items:** each field is cut, items are
  capped, and the result says there are more.
- **An agent asking to add a feed that already exists, or past the
  per-owner limit:** refused before any approval is asked.
- **A feed with no dates:** items are returned in feed order; a "since"
  filter skips items without a date.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The operator MUST be able to add, list and remove feeds
  for any user or the Household, by address and name.
- **FR-002**: Feeds MUST belong to exactly one owner. A personal agent
  MAY read its owner's and the Household's feeds; the Household agent
  reads only the Household's. Scope MUST come from the agent's owner,
  never from an argument.
- **FR-003**: Feed addresses MUST be http or https, with a host, no
  embedded credentials, and at most 2,000 characters.
- **FR-004**: Only the gateway fetches feeds. It MUST refuse to connect
  to loopback, private, link-local, shared (CGNAT), multicast,
  unspecified, broadcast, reserved and documentation addresses, IPv4 and
  IPv6 (including IPv4 written inside IPv6), and MUST check every
  address a host name resolves to, again on every redirect.
- **FR-005**: Fetches MUST be limited: at most 3 redirects, 2 MB, and
  10 seconds; and MUST NOT go through any proxy.
- **FR-006**: Each feed MUST be fetched at most once per 15 minutes
  (failures at most once a minute), whichever agent asks.
- **FR-007**: The gateway MUST read RSS 2.0, RSS 1.0 and Atom, and
  return each item's title, link, date and summary as plain text, with
  markup removed, each field cut to a limit, items capped per call, and
  the whole result capped.
- **FR-008**: Feed content MUST be treated as untrusted data: never as
  instructions or configuration, and never able to add feeds or change
  what any agent can do.
- **FR-009**: An agent's request to add a feed MUST need the owner's
  approval (the Household's goes to every user), showing the exact
  address and name. Invalid, internal, duplicate or over-limit requests
  MUST be refused before anyone is asked.
- **FR-010**: Errors MUST NOT reveal internal addresses or the gateway's
  network.
- **FR-011**: Every feeds call, every operator change, and every
  approved add MUST be audited, with the owner.
- **FR-012**: Doctor MUST report how many feeds each owner has, and warn
  when a feed failed on its last fetch.
- **FR-013**: Tests MUST use a fake feed server, never the internet. Any
  allowance that lets the gateway reach the fake (a private address)
  MUST exist only in stub and eval set-ups, and only for named Compose
  services.
- **FR-014**: Negative tests MUST cover: another owner's feed; an
  argument naming a user; each refused address range; a redirect to a
  private address; an oversized feed; an add without approval.
- **FR-015**: The eval suite MUST gain the tasks in User Story 5.

### Key Entities

- **Feed:** an owner, a short name (unique per owner), an address, and
  who added it (the operator, or an agent with an approval) and when.
- **Feed copy:** the gateway's in-memory copy of a fetched feed, or of a
  failure, with when it was fetched.
- **Feed item:** title, link, date and summary, as plain text.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of cross-owner reads in tests are refused.
- **SC-002**: 100% of the refused address ranges and private redirects
  in tests are refused, with no connection made to them.
- **SC-003**: An agent-requested feed is added only after approval, and
  exactly once, in 100% of tests.
- **SC-004**: No agent result, log or audit row contains an internal
  address of the stack.
- **SC-005**: A feed read twice within 15 minutes is fetched from the
  publisher once.
- **SC-006**: The full test suite passes in CI with no internet.

## Assumptions

- **Where feeds live:** in a `feeds` table in the gateway's own
  Postgres database, written only by the gateway. The operator's CLI
  changes feeds through the gateway's admin endpoints (like `sesh google
  connect`), so the gateway records every change in the audit log, and
  agent-requested adds, once approved, are written the same way.
  (The suggested `gateway/feeds.toml` was not used: the gateway mounts
  its directory read-only, and two stores would split one list; see
  research.md R1.)
- **Names:** `[a-z0-9][a-z0-9-]{0,31}`; without `--name`, the first
  label of the host (`www.` dropped).
- **Limits:** 50 feeds per owner; 25 items per read (10 by default);
  titles 300 characters, summaries 1,000, links 2,000; results 20 kB.
- **The network:** the gateway already reaches the internet on the
  stack's `default` network (for weather, maps and Google); feeds use
  it, guarded by the address filter. No new Compose network.
- **No discovery:** the address must be the feed itself; sesh doesn't
  find a feed from a web page.
- **Formats:** RSS 2.0, RSS 1.0 (RDF) and Atom 1.0, UTF-8 only.
  Podcasts' enclosures and images are ignored.
- **Out of scope:** OPML import; full-article fetching (the browser's
  `read(url)`, feature 010); push (WebSub); per-item read state.
- **Roadmap:** this is feature 006; read-only Google Docs becomes 007,
  and memory follows (008).
