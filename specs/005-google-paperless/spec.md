# Feature Specification: Google and Paperless, with per-user identity

**Feature Branch**: `claude/sharp-ramanujan-6b9nht`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Phase 5 — Google and Paperless, with per-user identity (Solutions Architecture phase 5; constitution III, IV, V; ADRs 0004, 0007). Per-user identity: agent records gain an owner (a household user, or Household); the gateway derives every scope from the agent's token, never from the model; operator commands to add users and bind agents to them. Google connector (Gmail search/read/draft/send, Calendar list/create, Contacts search) through Google's APIs with an OAuth consent flow: the operator asks for a consent link, the person opens it and consents, the refresh token goes straight to the secret store, scoped to that user; sends and event creation are approval-gated; read-only + compose scopes. Paperless-ngx as a new adopted service with one Paperless user per user scope; `docs` connector: paperless.search, paperless.get, paperless.upload(ref, {tags, title}) where ref is a gateway-held id (a Gmail attachment) so file bytes never enter model context; household uploads notify. Negative tests: user A's agent can't reach user B's mail or documents; send without approval never runs; no OAuth token in any output. Tests use fake Google APIs (the stub) and the real Paperless image in CI with no internet; eval suite gains tasks for mail and documents against fakes. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **The operator** runs sesh.
- **Household users** each have their own mail, calendar and documents.
- **The Household** is the shared identity, with shared accounts.
- **Agents** act for one owner. In this phase the test client and the
  eval play them.

### User Story 1 - Every agent belongs to someone (Priority: P1)

The operator adds household users, then gives each agent an owner: one
user, or the Household. From then on, everything an agent reaches is
decided by whose agent it is:
- whose mail, calendar and documents;
- whose credentials;
- who is asked for approval.

The model never names a user, and can't widen its reach by trying.

**Why this priority**: Constitution IV. Without an owner on every agent,
connecting a person's mail would expose it to every agent.

**Independent Test**: Add users Ana and Ben, and an agent for each. Ana's
agent reaches only Ana's (fake) mailbox and documents. A script that
names Ben, or passes a user or account argument, still gets only Ana's
data, or is refused.

**Acceptance Scenarios**:

1. **Given** users Ana and Ben with one agent each, **When** Ana's agent
   searches mail, **Then** only Ana's messages come back.
2. **Given** Ana's agent, **When** a script tries to choose a user,
   account or mailbox by argument, **Then** the call is refused as
   outside the method's arguments, and nothing of Ben's is returned.
3. **Given** a Household agent, **When** it searches mail, **Then** it
   sees only the Household account, never a personal one.
4. **Given** an agent whose owner is removed, **When** it calls any
   connector, **Then** the call is refused, and the agent's token stops
   working.
5. **Given** a gated action by Ana's agent, **When** the approval is
   sent, **Then** it goes to Ana only. A Household action goes to every
   user, and any one of them may decide.

---

### User Story 2 - Connecting a Google account without handing it over (Priority: P1)

The operator asks sesh to connect Ana's Google account. sesh gives a
one-time consent link. Ana opens it, signs in to Google herself, and
grants sesh the limited access it asks for:
- read mail;
- draft and send mail;
- manage calendar events;
- read contacts.

The long-lived grant goes straight to the secret store, under Ana's
name. No person types it, and no agent ever sees it. Ana can disconnect
at any time, and sesh then forgets the grant.

**Why this priority**: Constitution III. The rest of the feature depends
on a safe way to connect accounts.

**Independent Test**: Against a fake Google consent service:
- The link works once.
- After consent, Ana's agent can search Ana's mail.
- The grant appears in no output, log, audit row or file.
- After disconnecting, calls answer "not connected".

**Acceptance Scenarios**:

1. **Given** the operator requests a consent link for Ana, **When** sesh
   responds, **Then** it shows a one-time link, and never asks for or
   shows any Google credential.
2. **Given** Ana completes consent, **When** sesh finishes, **Then** the
   grant is stored under Ana's identity only, and the link stops working.
3. **Given** consent that names fewer permissions than sesh asked for,
   **When** sesh finishes, **Then** it stores the grant, and reports which
   abilities are unavailable. Methods that need them answer "not
   permitted".
4. **Given** Ana isn't connected, **When** her agent calls a mail method,
   **Then** the call answers "Google isn't connected for this user".
   Doctor names her as not connected.
5. **Given** Ana disconnects, **When** her agent next calls Google,
   **Then** it answers "not connected", and the stored grant is gone.

---

### User Story 3 - Mail and calendar, with sends approved (Priority: P1)

Ana's agent can search and read her mail, list her calendar, and search
her contacts on its own. It can write a draft. Sending a message or
creating an event waits for Ana's yes. The approval says:
- for a send: who the message goes to, its subject, and the start of the
  body;
- for an event: its title, time and guests.

Once Ana approves, it happens exactly once.

**Why this priority**: This is the first resource with real side
effects, and the first a person will use daily.

**Independent Test**: Against fake Google APIs:
- Search and read return fake messages.
- A draft is created with no approval.
- A send waits for approval. Approve, then re-run: the fake records one
  sent message.
- Deny: none is sent.
- An event creation behaves the same way.

**Acceptance Scenarios**:

1. **Given** Ana's agent, **When** it searches and reads mail, **Then**
   it gets messages marked as untrusted content. Attachment entries come
   back as references, never their bytes.
2. **Given** a send, **When** it runs without approval, **Then** nothing
   is sent, Ana is notified with the recipients, subject and opening of
   the body, and the agent is told approval is pending.
3. **Given** Ana approves, **When** the agent re-runs the same send,
   **Then** exactly one message is sent. A changed recipient or body
   needs a new approval.
4. **Given** a calendar event with guests, **When** it is created,
   **Then** the same approval rule applies.
5. **Given** a message whose body tells the agent to send something,
   **When** the agent acts on it, **Then** the send still waits for Ana,
   and the approval text shows exactly what would be sent.

---

### User Story 4 - Documents, filed without the model reading the file (Priority: P2)

Each user has their own document store, and the Household has a shared
one.
- **Reading:** an agent can search and read the text of its owner's
  documents. A personal agent can also read the Household's.
- **Filing:** an agent can file an email attachment into its owner's
  documents by referring to it. It never handles the file itself; the
  gateway moves the bytes.
- **Household filing:** filing into the Household's documents tells every
  user.

**Why this priority**: Documents are valuable, but they depend on users
existing (US1), and on mail for the main way documents arrive.

**Independent Test**: With the real document service:
- Ana's agent files a fake attachment by reference. The document appears
  in Ana's store with the given title and tags.
- The file's bytes never appear in any agent result.
- Ben's agent can't find it.
- A Household upload notifies both users.

**Acceptance Scenarios**:

1. **Given** an attachment reference from Ana's mail, **When** Ana's
   agent files it with a title and tags, **Then** it lands in Ana's
   documents, and the result says only that it was filed, with the new
   document's id.
2. **Given** that reference, **When** Ben's agent tries to file it,
   **Then** it is refused: references belong to the agent that received
   them.
3. **Given** Ana's documents, **When** Ben's agent searches, **Then** it
   finds none of them. It finds the Household's shared documents.
4. **Given** the Household agent files a document, **When** it is filed,
   **Then** every user is notified with its title.
5. **Given** a file of a disallowed type, or over the size limit, **When**
   it is filed, **Then** it is refused, and nothing is stored.

---

### User Story 5 - Evals cover mail and documents (Priority: P3)

The eval suite gains tasks that use the new resources, against fakes:
- find a fact in an email and cite its subject;
- draft a reply, which needs no approval;
- send only with approval;
- file an attachment by reference, without pasting its contents.

**Why this priority**: The eval should tell the operator whether a model
can handle these resources. It follows the resources themselves.

**Independent Test**: Against the stub model's passing script, the new
tasks pass. Against the failing script, they fail for the expected
reasons. One of those reasons is that the model tried to paste file
contents instead of filing by reference.

**Acceptance Scenarios**:

1. **Given** the eval, **When** it runs, **Then** it uses fake mail and
   documents, never a real account.
2. **Given** a model that pastes attachment contents into its answer,
   **When** the filing task is scored, **Then** it fails with that reason.

### Edge Cases

- **A Google grant is revoked from Google's side:** the next call answers
  "Google access was revoked; reconnect", and doctor flags it.
- **Two agents with the same owner:** both see that owner's data, and
  approvals go to the owner. Every grant belongs to the owner, not the
  agent.
- **An attachment reference that is expired, unknown or another
  agent's:** refused, with the reason.
- **A very large mailbox search:** results are capped, with an
  indication that there are more.
- **The document service is down:** document methods answer
  "unavailable"; mail still works.
- **A user removed with documents and a grant:** the grant is deleted,
  and the documents are kept but unreachable by any agent until the
  operator deals with them.
- **An approval for a Household send:** the first user to decide
  settles it; a later decision is refused as already decided.
- **A consent link opened on a device off the private network:** the
  link must still reach sesh, so the consent step is served where the
  person's browser can reach it (Assumptions).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The operator MUST be able to add, list and remove household
  users. There is one Household identity, which always exists.
- **FR-002**: Every agent MUST have exactly one owner: a user, or the
  Household. Agents without an owner MUST NOT be accepted.
- **FR-003**: Every connector call MUST be scoped to the owner of the
  agent whose token made it. The model MUST NOT be able to name or
  change the user, account or store by any argument.
- **FR-004**: Approval requests MUST go to the owner's approval channel.
  Household requests go to every user's, and any one user's decision
  settles them.
- **FR-005**: The operator MUST be able to request a one-time consent
  link to connect a user's (or the Household's) Google account, and to
  disconnect it.
- **FR-006**: The long-lived Google grant MUST go straight from the
  consent step to the secret store under that user's identity. It MUST
  NOT be shown to the operator, typed by a person, or reachable by any
  agent, log, audit row or file.
- **FR-007**: Google access MUST be limited to:
  - reading mail;
  - composing and sending mail;
  - managing calendar events;
  - reading contacts.

  Nothing broader may be requested.
- **FR-008**: The Google connector MUST offer mail search, read and
  draft; mail send; calendar list and create; and contact search.
  Sending and event creation MUST need approval.
- **FR-009**: Approval text for a send MUST show the recipients, subject
  and the opening of the body. For an event, it MUST show the title,
  time and guests.
- **FR-010**: Mail content and attachment names returned to agents MUST
  be marked untrusted. Attachment bytes MUST NOT be returned; only
  references are.
- **FR-011**: An attachment reference MUST be usable only by the agent
  that received it, and only until it expires.
- **FR-012**: The document service MUST keep each user's documents, and
  the Household's, separate.
  - A personal agent MAY read its owner's and the Household's documents,
    and MUST file only to its owner's.
  - The Household agent reads and files only the Household's.
- **FR-013**: Filing a document by reference MUST:
  - move the bytes inside the trusted plane only;
  - limit file type and size;
  - return only the new document's id and title.
- **FR-014**: Filing to the Household's documents MUST notify every user.
- **FR-015**: Every Google and document call, consent event, disconnect
  and filing MUST be audited, with no grant, credential or file bytes in
  the entry.
- **FR-016**: Doctor MUST report each user's Google connection state, and
  whether the document service is healthy.
- **FR-017**: Tests MUST use fake Google services, and the real document
  service, with no internet.
- **FR-018**: Negative tests MUST cover:
  - cross-user mail and documents;
  - an argument that names a user or account;
  - a send without approval;
  - another agent's attachment reference;
  - a revoked grant;
  - a grant appearing in any output;
  - a disallowed file type.
- **FR-019**: The eval suite MUST gain the mail and document tasks in
  User Story 5.

### Key Entities

- **User:** a household member, with a name, an approval channel, a
  Google connection state and a document scope.
- **Household:** the shared identity, with the same parts.
- **Agent:** as in feature 002, plus its owner.
- **Google connection:** a user's grant in the secret store, with the
  abilities it allows and its state (connected, revoked, missing).
- **Consent link:** a one-time link for one user, with an expiry.
- **Attachment reference:** an opaque id the gateway issues to one agent
  for one attachment, with an expiry.
- **Document scope:** a user's or the Household's area in the document
  service.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A person connects their Google account in under 2 minutes,
  from opening the link.
- **SC-002**: 100% of the cross-user cases in FR-018 are refused, and
  appear in the audit log.
- **SC-003**: A send or event creation happens only after approval, and
  exactly once, in 100% of tests.
- **SC-004**: No Google grant, credential or attachment content appears
  in any agent result, log, audit row, eval output or data-directory
  file.
- **SC-005**: Filing an attachment by reference takes one call, and the
  document is searchable by its owner within 1 minute.
- **SC-006**: The full test suite passes in CI with no internet and no
  real Google account.

## Assumptions

- **Users:** in this phase they are records the operator creates (a
  name, and an approval topic each). Open WebUI accounts, Hermes profiles
  and `sesh user add` doing everything at once come in later phases.
  `sesh agent add` gains an owner.
- **Approval channels:** each user has their own ntfy topic, which
  replaces the single person topic from feature 004. The operator's
  existing set-up becomes the first user.
- **Google OAuth:**
  - The operator supplies the household's own Google OAuth client (a
    client id and secret from their Google Cloud project), entered
    through a one-time credential form (feature 004).
  - The consent callback is served by the gateway at the approvals
    address. For a phone, that's the Tailscale HTTPS URL, which Google
    accepts as a redirect.
  - The Google app being "in production" but unverified shows a one-time
    warning (an accepted risk in the architecture).
- **Google access level:** Gmail read-only plus compose (for drafts and
  sends), Calendar events, and Contacts read-only.
- **Documents:** Paperless-ngx, self-hosted, with its own database in
  the existing Postgres and its own Redis database. There is one
  Paperless user per sesh user, plus the Household. The gateway uses a
  Paperless admin token, kept in the secret store, and acts as the right
  user per call.
- **Limits:**
  - file types: PDF, images (PNG, JPEG, TIFF) and Office documents;
  - files up to 25 MB;
  - attachment references expire after 1 hour;
  - mail searches return up to 25 results;
  - Paperless's own document processing (OCR) may take a little while
    after filing.
- **Out of scope:** OCR or semantic search beyond what Paperless
  provides; Paperless-AI; the mail rule that imports a Gmail label
  (operator configuration in Paperless itself); live Google in CI.
