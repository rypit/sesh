# Feature Specification: Browser, with a live view and takeover in the chat

**Feature Branch**: `claude/browser`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Proceed with 010 (browser). Browser usage is critical. I don't want to use a one-time link to see the session live and take over, I want that experience integrated into Open WebUI. If an agent uses a browser, I want a small window to show in chat that shows the browser. Clicking it would then launch the browser in a larger window and controls to take over would be shown. When the user closes the browser, its control is returned to the agent." Architecture phase 10: a persistent browser profile per agent, `read(url)` and `login(site)`; accept: `read(url)` returns Markdown and refuses a LAN address, `login(site)` refuses on a lookalike domain, the agent can't read a filled password field, the live window shows the session, takeover pauses the agent and closing returns control, another user can't open the view.

## User Scenarios & Testing *(mandatory)*

Actors:
- **A household member** chats with an agent in Open WebUI and watches
  or takes over its browser.
- **The agent** (untrusted) browses on that person's behalf, only
  through the gateway.
- **An attacker** controls a web page the agent visits, or the model's
  output, and tries to reach the home network, steal a login, or act
  without the person.

### User Story 1 - The agent reads a web page (Priority: P1) 🎯 MVP

A household member asks "what does this page say?" or "summarize
example.org/article". The agent reads the page through the gateway and
answers from its text.

**Why this priority**: Reading the web is the most common browser use,
and needs no login or live view.

**Independent Test**: In stub mode, the agent's `read` of a fake page
returns its main text as Markdown, labelled as untrusted; a `read` of a
private, LAN, Tailscale or loopback address is refused, including when
a public address redirects there.

**Acceptance Scenarios**:

1. **Given** a public page, **When** the agent reads it, **Then** it
   gets the page's main text as Markdown, capped in size, marked as
   untrusted content.
2. **Given** an address on the home network (private, LAN, Tailscale,
   loopback or link-local), **When** the agent reads it, **Then** the
   read is refused and nothing is fetched from it.
3. **Given** a public page that redirects to a home-network address,
   **When** the agent reads it, **Then** the redirect is refused.
4. **Given** a page that takes too long or is too large, **When** the
   agent reads it, **Then** the read stops at the limit and says so.

---

### User Story 2 - The person sees the agent's browser in the chat and can take over (Priority: P1)

When the agent opens the browser, a small live window appears in its
chat message, showing the page as it changes. The person clicks it and
a larger window opens with controls; they click and type in the page
themselves while the agent waits. When they close the larger window,
the agent carries on from where they left it.

**Why this priority**: The household's explicit requirement, and what
makes logins, captchas and checkouts possible without giving the agent
a password.

**Independent Test**: In stub mode, a chat where the agent opens a fake
page shows a live window in the Open WebUI message; opening the larger
window and taking over makes the agent's next browser call wait; input
sent in the larger window reaches the page; closing it releases the
agent, whose next call sees the page as the person left it.

**Acceptance Scenarios**:

1. **Given** a chat in which the agent opens the browser, **When** the
   message is shown in Open WebUI, **Then** it contains a small live,
   view-only window of the agent's browser.
2. **Given** the small window, **When** the person clicks it, **Then** a
   larger window opens with the live page and controls (take over,
   hand back, close).
3. **Given** the person has taken over, **When** the agent makes a
   browser call, **Then** the call waits (up to a limit) and the agent
   is told the person is in control if the limit passes.
4. **Given** the person is in control, **When** they click or type in
   the larger window, **Then** the page receives it.
5. **Given** the person closes the larger window or hands back,
   **When** the agent's next browser call runs, **Then** it acts on the
   page as the person left it.
6. **Given** another household member, or anyone without the message,
   **When** they open the live view's address, **Then** they see
   nothing and can't send input.
7. **Given** the agent, **When** it looks at the page after a takeover,
   **Then** it can't read what the person typed into fields.

---

### User Story 3 - The agent logs in to a site without seeing the password (Priority: P1)

The person stores a login for a site once, through the one-time
credential form. When the agent needs it, it asks the gateway to log
in; the gateway checks that the browser is really on that site, fills
the fields itself, and tells the agent only whether it worked.

**Why this priority**: Logged-in browsing is the point of a persistent
profile, and the password must never reach the model.

**Independent Test**: In stub mode, with a stored login for a fake site,
`login` on the real fake site succeeds and the agent's view of the page
shows the password field masked; `login` while the browser is on a
lookalike domain is refused and nothing is filled; the first login to a
site waits for the person's approval.

**Acceptance Scenarios**:

1. **Given** a stored login for a site, **When** the agent asks to log
   in while the browser is on that site, **Then** the gateway fills the
   username and password (and a one-time code, if one is stored) and
   reports only success or failure.
2. **Given** the browser is on a lookalike or different domain, **When**
   the agent asks to log in, **Then** it is refused and nothing is
   filled.
3. **Given** a site the agent has never logged in to, **When** it asks
   to log in, **Then** the person is asked to approve first.
4. **Given** a filled password field, **When** the agent looks at the
   page, **Then** the value is masked.

---

### User Story 4 - The agent browses step by step (Priority: P2)

The agent opens a page, looks at it, clicks a link or button and types
into a search box, keeping cookies and logins between chats in its own
profile.

**Why this priority**: Needed for anything beyond reading a single
page, but each step builds on Stories 1–3.

**Independent Test**: In stub mode, the agent opens a fake site,
snapshots it, follows a link by its reference, types into a field and
submits; submitting a form asks the person first; cookies persist
across a restart of the browser.

**Acceptance Scenarios**:

1. **Given** an open page, **When** the agent snapshots it, **Then** it
   gets the visible text and a numbered list of the links, buttons and
   fields, with field values masked.
2. **Given** a snapshot, **When** the agent clicks an element by its
   number or types into a field, **Then** the page responds and the
   agent can snapshot again.
3. **Given** a click that would submit a form, **When** the agent makes
   it, **Then** the person is asked to approve first.
4. **Given** the agent's profile, **When** the browser restarts,
   **Then** its cookies and logins are still there; another agent's
   profile is never visible to it.

### Edge Cases

- **The person never hands back**: the agent's browser calls time out
  with "the person is in control"; closing the window, or a takeover
  limit (default 15 minutes), hands control back.
- **The live window is opened on a phone**: it works over the same
  private address as approval links (Tailscale); on a small screen the
  larger window fills the screen.
- **Two chats with the same agent**: one browser per agent; the live
  window appears in the chat whose request opened or used the browser.
- **The agent tries JavaScript, file:// or chrome:// addresses, or
  downloads**: refused; downloads are out of scope for this feature.
- **A page tries to reach the home network** (images, scripts,
  redirects, WebSockets): blocked like navigation.
- **The browser crashes or is restarted**: the next call starts it
  again with the same profile; the live window shows that it
  reconnected.
- **A live view link leaks** (copied out of the chat): it expires
  quickly and only works for the person it was issued to.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The gateway MUST offer a `browser` connector with `read`,
  `open`, `snapshot`, `click`, `type` and `login`; agents reach the
  browser only through it.
- **FR-002**: `read(url)` MUST return the page's main text as Markdown,
  labelled as untrusted, within size and time limits, using a fresh
  browsing context that shares nothing with the agent's profile.
- **FR-003**: Every request the browser makes (navigation, redirects,
  subresources, WebSockets) MUST be refused if it goes to a private,
  LAN, Tailscale, loopback, link-local or metadata address, checked on
  the resolved address.
- **FR-004**: Each agent MUST have its own persistent browser profile;
  no agent can see another's.
- **FR-005**: The agent MUST NOT be able to run its own JavaScript,
  read field values, open `file:`, `chrome:` or other non-web
  addresses, or reach the browser except through the connector.
- **FR-006**: `login(site)` MUST fill stored credentials only when the
  browser's current address belongs to the site the credential was
  saved for, MUST report only success or failure, and MUST ask the
  person before the first login to each site.
- **FR-007**: A click that submits a form MUST wait for the person's
  approval (ADR 0007 rules).
- **FR-008**: When an agent's browser is in use during a chat, the
  agent's message in Open WebUI MUST show a small live, view-only
  window of the browser.
- **FR-009**: Opening the small window MUST show a larger live window
  with controls to take over, hand back and close.
- **FR-010**: While the person is in control, the agent's browser calls
  MUST wait (up to a limit) and then fail with "the person is in
  control"; handing back or closing MUST return control.
- **FR-011**: The live view and its input MUST be available only to
  the Open WebUI user the agent's message was for, with a short-lived
  token the agent never sees; view and input MUST pass through the
  gateway.
- **FR-012**: What the person types during a takeover MUST NOT be
  shown to the agent.
- **FR-013**: Every browser call, takeover and hand-back MUST be in
  the audit log, without page content, typed text or credentials.
- **FR-014**: The browser MUST run isolated from the household
  services, with no network route except through the gateway's checks,
  and `sesh doctor` MUST check it.
- **FR-015**: Stack tests MUST cover every acceptance scenario with fake
  sites and no internet, including the negative ones (home-network
  addresses, lookalike domains, another user's view, masked fields).
- **FR-016**: The decisions MUST be recorded in ADRs (a new adopted
  component, its isolation, the gateway in the chat path, the live
  view's access rule), with the architecture document and diagram
  updated in the same change.

### Key Entities

- **Browser session**: an agent's running browser, its profile, its
  current page, and who is in control (the agent or a person).
- **Live view grant**: permission for one Open WebUI user to watch, and
  take over, one agent's browser for a short time.
- **Site login**: a stored credential with the site it belongs to, and
  whether the person has approved its first use.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the stub-mode stack test, a chat where the agent uses
  the browser shows a live window in the Open WebUI message within 5
  seconds of the browser opening.
- **SC-002**: 0 requests from the browser reach a home-network address,
  across every probe in the test (direct, redirect, subresource).
- **SC-003**: 0 credential values appear in anything the agent
  receives, the audit log or any log, across the test.
- **SC-004**: During a takeover, 100% of the agent's browser calls wait
  or fail with "the person is in control"; after hand-back the next
  call succeeds.
- **SC-005**: Another user's attempt to open or drive the live view
  fails every time.
- **SC-006**: Every existing stack test still passes.

## Assumptions

- **The live window is an Open WebUI message embed** (an iframe Open
  WebUI shows in the message), served by the gateway over the same
  private address as approval pages. Open WebUI is not modified.
- **The browser is Chromium**, driven by the gateway; downloads and file
  uploads are later work.
- **Isolation is a hardened container per agent**; a microVM (sbx) is
  deferred, because it needs a Docker account sign-in and can't run in
  CI (research.md).
- **Only agents with a runtime** (feature 009) get the live window; the
  connector works for any agent granted `browser`.
- **Per-user agents** come in 012; until then the Household agent's live
  window is for the household member who is chatting.
