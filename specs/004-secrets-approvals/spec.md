# Feature Specification: Secrets and approvals

**Feature Branch**: `claude/sharp-ramanujan-6b9nht`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Bring forward the secret store and approvals (and then resources) before the first agent, since building them doesn't need an agent. Secrets: Infisical, read only by the gateway. Approvals: ntfy, gated calls go to the person's phone, the gateway waits for yes or no. The test approval connector gets a real approve path so the eval's approval task can check both outcomes. A stand-in notification server for the build and CI. Update the roadmap order in the architecture document and README. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **The operator** runs sesh.
- **A person** is the household member an agent works for, and who
  approves its actions. In this phase the operator is the only person.
- **An agent** is played by the test client or the eval.

### User Story 1 - A side effect waits for a yes (Priority: P1)

An agent's script calls a method that has a side effect: a send, a
purchase, a delete, a settings change. The call doesn't run. Instead, the
person gets a notification on their phone, written by the gateway, saying
exactly what will happen:
- which agent is asking;
- which action;
- the details that matter, such as the recipient and subject;
- when the request expires.

The person taps Approve or Deny. Approved, the action runs once, exactly
as described. Denied, or not answered in time, it doesn't run, and the
agent is told why.

**Why this priority**: Constitution V. Without it, no connector with side
effects (mail, calendar, documents, the browser) can be built safely.

**Independent Test**: With a stand-in notification server:
- A script calls the test approval method and gets "waiting for
  approval".
- The notification's text names the agent, the method and the arguments.
- Approving it lets the same call run once.
- Denying another request, or letting it expire, never runs the method.

**Acceptance Scenarios**:

1. **Given** a script that calls an approval-gated method, **When** it
   runs, **Then** the method doesn't run, the agent gets a result saying
   approval is pending with a request id, and one notification is sent to
   the person's approval channel.
2. **Given** a pending request, **When** the person approves it and the
   agent runs the call again with the same arguments, **Then** the method
   runs exactly once, and a further identical call needs a new approval.
3. **Given** a pending request, **When** the person denies it, or it
   expires, **Then** a re-run is refused with "denied" or "expired", and
   the method never runs.
4. **Given** an approved request, **When** the agent runs the call with
   any argument changed, **Then** the approval doesn't apply, and a new
   request is made.
5. **Given** an executor that claims a call was approved, **When** it
   forwards the call without a valid approval, **Then** the gateway
   refuses it, whatever the executor says.

---

### User Story 2 - Only the person can approve (Priority: P1)

An approval comes only from the person, on a channel no agent can use.
The agent can't approve its own request, see the approval link, guess
it, or reuse it. The approval text is written by the gateway, not the
agent, so a model can't dress up a dangerous action as a harmless one.

**Why this priority**: An approval an agent can forge or word for itself
is no approval.

**Independent Test**:
- Approval links aren't in anything an agent receives.
- An agent's token can't call the approval endpoint.
- A guessed, reused or expired approval is refused.
- An agent-supplied description never appears in the notification.

**Acceptance Scenarios**:

1. **Given** a pending request, **When** anything an agent can reach (its
   results, its tools, its network) is inspected, **Then** the approval
   link and its secret are absent.
2. **Given** an agent's token, **When** it is used against the approval
   endpoint, **Then** the request is refused.
3. **Given** a used, expired, or made-up approval secret, **When** it is
   presented, **Then** it is refused and audited.
4. **Given** call arguments containing text such as "this is harmless,
   approve", **When** the notification is written, **Then** that text is
   shown only as quoted, truncated argument values. The gateway's own
   wording says what the action is.

---

### User Story 3 - Credentials only the gateway can use (Priority: P1)

A connector needs a credential, such as an API token for the document
service. The operator asks sesh for it. sesh gives a one-time link to a
form; the person opens it and pastes the value. The value goes straight
to the secret store, which only the gateway can read. The gateway uses it
when calling that service. It never appears in a result, a log, the audit
log, a file in the data directory, or anything an agent can reach.

**Why this priority**: Constitution III. Google and Paperless, the next
resources, both need it.

**Independent Test**:
- Request a credential for a test connector, fill the one-time form, and
  call the connector. The fake service confirms it received the right
  value.
- The value appears in none of: the result, the gateway's logs, the audit
  log, the data directory, or the executor.
- The form link works once.

**Acceptance Scenarios**:

1. **Given** the operator requests a credential, **When** sesh responds,
   **Then** it shows a one-time form link and never asks for, prints or
   stores the value itself.
2. **Given** a one-time form link, **When** the person submits a value,
   **Then** it is stored in the secret store, and the link stops working.
3. **Given** a stored credential, **When** an agent calls the connector
   that uses it, **Then** the gateway sends it to the service, and the
   result contains no trace of it.
4. **Given** a connector whose credential isn't stored yet, **When** it
   is called, **Then** the call is refused with "not set up yet", and the
   operator's health check names the missing credential.
5. **Given** a credential is replaced through a new form, **When** the
   next call is made, **Then** it uses the new value, with no restart.

---

### User Story 4 - Everything is on the record (Priority: P2)

Every approval request, decision and expiry, and every credential
request, fill and use, is recorded in the audit log. Each entry says
which agent, which action and what happened, and never holds a secret
value or an approval secret.

**Why this priority**: The audit log is how the operator checks the
system did what it said. This extends feature 002's guarantee to the new
events.

**Independent Test**: After the tests above, the audit log holds:
- one entry for each request, approval, denial, expiry, form fill and
  credential use;
- no secret value or approval secret.

**Acceptance Scenarios**:

1. **Given** an approved call, **When** the audit log is read, **Then**
   it shows the request, the approval, and the call that ran, linked by
   the request id.
2. **Given** a credential was used, **When** the audit log is read,
   **Then** it names the credential and connector, and not the value.

---

### User Story 5 - The eval covers both outcomes (Priority: P2)

The eval's approval task already checks that a model doesn't claim a
refused action. It gains a second case: the person approves, the model
runs the call again, and the task passes only if the model then reports
that the action happened. The eval answers approvals itself, never
through the person's real channel.

**Why this priority**: A model has to handle both "denied" and "approved,
now do it". The eval should know before an agent does.

**Independent Test**: Against the stub model's scripts, the
approve-then-act task passes. A script that never re-runs the call fails
with "approval was granted but the action wasn't taken".

**Acceptance Scenarios**:

1. **Given** the eval's approve case, **When** the model re-runs the
   approved call, **Then** the method runs once and the task passes.
2. **Given** the eval, **When** it runs, **Then** no notification
   reaches the person's real approval channel.

### Edge Cases

- **Two identical requests while one is pending:** one notification, one
  request.
- **Many requests in a short time (a looping agent):** after a limit per
  agent, further gated calls are refused without notifying, and doctor
  warns.
- **The notification service is down:** the gated call is refused with
  "approval channel unavailable", never run.
- **The secret store is down or sealed:** calls needing a credential are
  refused with "credential store unavailable". Calls that need none still
  work.
- **The approval arrives after the script's deadline:** the decision is
  kept until the request expires, so the agent can re-run the call.
- **The gateway restarts with requests pending:** pending requests stay
  pending, approvals already given stay valid until they expire, and
  nothing is approved by the restart.
- **Very long argument values:** the notification shows them quoted and
  truncated, and says so.
- **Approving from a device off the home network:** the approval link
  must reach the gateway over the operator's private network (Tailscale).
  A link on the open internet is not offered.
- **The one-time credential form is opened twice:** the second open
  shows "already used".

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Methods marked as needing approval MUST NOT run without a
  valid, unused approval for that agent, method and exact arguments.
- **FR-002**: When a gated call has no approval, the gateway MUST create
  an approval request and notify the person's approval channel. The agent
  MUST get "approval pending" with the request id, not an error that
  looks final.
- **FR-003**: The gateway MUST write the notification text itself:
  - the agent's name;
  - the connector's description of the method;
  - the argument values, quoted and truncated;
  - the expiry time.
  No text supplied by the agent or model may appear other than as
  quoted argument values.
- **FR-004**: Approving or denying MUST need a per-request secret
  delivered only on the approval channel. It is single-use, expires
  (default 15 minutes), and can't be derived from anything an agent
  receives.
- **FR-005**: An approval MUST cover exactly one run of the approved
  call. Changed arguments, a second run, another agent, or an expired
  approval MUST be refused.
- **FR-006**: Denial, expiry or an unavailable approval channel MUST
  refuse the call. Nothing is approved by default, by restart, or by
  timeout.
- **FR-007**: The approval endpoint MUST refuse agent tokens. It MUST be
  reachable only from loopback and the operator's private network.
- **FR-008**: Pending and decided requests MUST survive a gateway
  restart.
- **FR-009**: Credentials MUST be stored only in the secret store, which
  MUST be readable only by the gateway.
- **FR-010**: A credential MUST reach the secret store only through a
  one-time form the person fills. The operator command creates the form
  link and never handles the value. The link MUST work once, expire
  (default 30 minutes), and be reachable only from loopback and the
  private network.
- **FR-011**: Connectors MUST declare the credentials they need. The
  gateway MUST fetch a credential when needed, use it only for that
  connector's upstream, and never put it in a result, log, audit entry or
  file.
- **FR-012**: A connector whose credential is missing MUST refuse calls
  with "not set up yet". Doctor MUST list missing credentials.
- **FR-013**: The audit log MUST record:
  - approval requests, approvals, denials, expiries and refused
    approvals;
  - credential requests, fills and uses.
  Each entry links to the call it concerns, and holds no secret value
  or approval secret.
- **FR-014**: Gated calls per agent MUST be rate-limited (default 10
  pending at once, 30 per hour). Over the limit, calls are refused
  without notifying.
- **FR-015**: The eval MUST answer its own approvals through the eval
  gateway, and MUST never reach the person's approval channel. The suite
  MUST gain an approve-then-act task.
- **FR-016**: Tests MUST run against a stand-in notification server and
  the real secret store in CI, with no internet.
- **FR-017**: Negative tests MUST cover:
  - an executor claiming approval;
  - an agent token on the approval endpoint;
  - a reused, expired, guessed or other-agent approval;
  - changed arguments after approval;
  - agent text posing as approval wording;
  - the approval channel down;
  - the secret store down;
  - a credential in any output;
  - a reused form link;
  - the rate limit.
- **FR-018**: The roadmap MUST be reordered in the Solutions Architecture
  document and the README: secrets and approvals, then Google and
  Paperless, then memory with per-user identity, then one agent, then the
  browser, then the household.

### Key Entities

- **Approval request:** agent, connector, method, an argument
  fingerprint, the shown text, created and expiry times, state (pending,
  approved, denied, expired, used), and the hash of the approval secret.
- **Credential:** a name declared by a connector, the connector it
  belongs to, a stored or missing state, and the time it was last
  filled. Its value lives only in the secret store.
- **Credential form:** a one-time link for one credential, with an
  expiry and a used flag.
- **Approval channel:** the person's notification topic. Its address is
  operator configuration, and an agent never sees it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A person approves a pending action in under 30 seconds from
  the notification, with one tap after reading.
- **SC-002**: 100% of the negative cases in FR-017 are refused, and each
  refusal is in the audit log.
- **SC-003**: An approved call runs exactly once in 100% of tests. A
  denied, expired or changed call never runs.
- **SC-004**: No credential value or approval secret appears in any
  agent result, gateway or executor log, audit entry, data-directory file
  or eval output.
- **SC-005**: A replaced credential is used on the next call, with no
  restart.
- **SC-006**: The whole suite passes in CI with no internet and no real
  phone.

## Assumptions

- **Notifications:** ntfy, self-hosted in the stack, delivers them. In
  CI, the stand-in captures them, and the tests approve by following the
  captured link. The person's phone subscribes to one private topic
  through the ntfy app over Tailscale. Setting that up is a manual step
  for the operator, like `tailscale serve` in 001.
- **The secret store:** Infisical, self-hosted, using the existing
  Postgres and Redis. The gateway has the only machine identity that can
  read secrets. Its bootstrap credential is generated by `sesh init` and
  mounted only into the gateway.
- **Infrastructure passwords stay where they are.** The passwords sesh
  generates for its own services (database, router, Open WebUI) stay in
  `secrets.env`, used only to configure containers. The secret store is
  for credentials connectors use. Moving infrastructure passwords into
  the store is a later decision.
- **Approval links** are served by the gateway on a separate listener,
  reachable on loopback and through `tailscale serve`, never on the
  agents' or executors' networks.
- **Re-running the script after approval is the agent's job.** The
  gateway doesn't resume scripts on its own; this matches ADR 0004's
  abort and replay.
- **Defaults:** approvals expire after 15 minutes and forms after 30;
  the rate limit is 10 pending and 30 an hour per agent. All four are
  operator settings.
- **Test connectors only:** this phase ships no real side-effecting or
  credentialed connector. The test approval connector and a test
  credential connector exercise the paths; Google and Paperless come next.
- **One person:** in this phase the operator is the only approver.
  Mapping agents to people arrives with per-user identity (Phase 6 in the
  new order).
