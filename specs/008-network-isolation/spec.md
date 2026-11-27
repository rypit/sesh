# Feature Specification: Network isolation

**Feature Branch**: `claude/network-isolation`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Feature 008 — network isolation: remove the shared Compose `default` network so each service reaches only what it needs (constitution I, II, VII; ADRs 0004, 0009, 0010). Today Postgres, LiteLLM, Open WebUI, the gateway, Infisical, ntfy and Paperless share `default`, so any of them can reach any other's ports. Replace it with: a database network per service to Postgres (litellm-db, openwebui-db, infisical-db, paperless-db, gateway-db; Miniflux's feeds-db already exists) so Postgres joins only those; pair networks for the remaining links (models: Open WebUI↔LiteLLM; secrets: gateway↔Infisical; notify: gateway↔ntfy); and one single-member edge network per service that publishes a loopback port or needs outbound access (Open WebUI, LiteLLM — inference server, gateway — Google APIs, Infisical, ntfy — phone wake-ups), because Docker only publishes ports for containers on a non-internal network. Existing internal networks (search, docs, feeds, feeds-egress, searxng-egress, per-service Redis networks, per-agent executor networks) stay as they are. The gateway's listener selection (agent :7000 and approvals :7002 bind loopback plus the default-route address) must follow the gateway's edge network, which is trusted-plane code and needs an ADR. Stub mode (stub-llm as model server, fake Google, fake feed publisher) and the eval project must keep working with no internet. A new stack test checks the full reachability matrix from every container (allowed paths as positive controls, everything else refused), using the network-namespace probe from feeds_stack; all existing stack tests must keep passing. `sesh up` on an existing install migrates by recreating containers; no data changes. Out of scope (record as assumptions/follow-ups): Postgres pg_hba rules binding each login to its network's subnet (needs fixed subnets); blocking internet egress for services whose edge network grants more than they need (e.g. Open WebUI, which stays in offline mode) — Compose can't express egress firewalls. Defaults: ntfy keeps an edge network (needed for its published port; also allows upstream push relays). Roadmap: this becomes 008; read-only Google Docs moves to 009, memory to 010, one agent 011, browser 012, household 013. Operator works from mobile: choose sensible defaults, record assumptions, no clarification questions."

## User Scenarios & Testing *(mandatory)*

Actors:
- **The operator** runs sesh, upgrades it, and reaches its web pages
  from the server or over the tailnet.
- **A household member** uses Open WebUI and approves actions from a
  phone.
- **An attacker** has taken over one adopted service (for example Open
  WebUI through a chat message, Paperless through a document, LiteLLM
  through a model reply) and tries to reach the rest of the stack from
  inside it.

Today seven services (the database, the model router, the chat UI, the
gateway, the secret store, the approval channel and the document
service) share one network, so each can open a connection to every
other's ports. Feature 007 confined each service's *data*; this feature
confines who can *talk to* whom.

### User Story 1 - A compromised service reaches only its own neighbours (Priority: P1)

Each service can open connections only to the services its job needs,
as listed in the reachability table below. A service that is taken
over can't open a connection to anything else in the stack: the chat UI
can't reach the secret store or the gateway's approval pages, the model
router can't reach the gateway, the approval channel can't reach
anything inside the stack, and only services that store data can reach
the database.

**Why this priority**: Constitution I (adopted services are untrusted)
and II (one way out, through the gateway). Lateral movement between
adopted services is the largest remaining path after feature 007.

**Independent Test**: In the running stack, try every pair of
(service, destination port) from inside each service's network
namespace. Every pair in the allowed list connects; every other pair is
refused.

**Acceptance Scenarios**:

1. **Given** the running stack, **When** each service tries to connect
   to each allowed destination in the reachability table, **Then**
   every one succeeds.
2. **Given** the running stack, **When** each service tries to connect
   to any destination not in its row, **Then** every one fails.
3. **Given** the running stack, **When** the database's network
   attachments are listed, **Then** it is attached only to the
   per-service database networks.
4. **Given** a service's database login and password, **When** they are
   used from anywhere but that service's own database network (another
   service, another network), **Then** the database refuses the login.
5. **Given** the database's administrative login, **When** it is used
   over the network from any container, **Then** it is refused; it works
   only from inside the database's own container.
4. **Given** the running stack, **When** the networks are listed,
   **Then** there is no network shared by more than the services its
   purpose names (no catch-all network).

---

### User Story 2 - Everything keeps working (Priority: P1)

After the change, the household uses sesh exactly as before: the chat
UI answers through the model router, agents reach the gateway, the
gateway reads secrets, sends approval notifications, searches, files
documents, reads feeds and calls Google, and the operator reaches the
published pages on the server and over the tailnet.

**Why this priority**: Isolation that breaks a path the household needs
would be undone in practice.

**Independent Test**: Every existing stack test (foundation, gateway,
evals, secrets and approvals, Google and documents, feeds, service
isolation) passes unchanged.

**Acceptance Scenarios**:

1. **Given** the new layout, **When** the existing stack tests run,
   **Then** all of them pass.
2. **Given** the new layout, **When** the operator opens each published
   page on the server's loopback address, **Then** it answers as
   before.
3. **Given** the new layout, **When** the doctor runs, **Then** every
   check that passed before still passes.

---

### User Story 3 - The upgrade is just `sesh up` (Priority: P1)

An operator on an earlier version runs `sesh up` with this version.
Containers are recreated on their new networks, the old shared network
is removed, and no data, account, credential or document is lost.

**Why this priority**: Upgrades must not need manual network work or
put data at risk.

**Independent Test**: Start the stack as it is before this feature,
create data in each service, run the new `sesh up`; the data is there,
the old shared network is gone, and the reachability check of story 1
passes.

**Acceptance Scenarios**:

1. **Given** a running pre-008 stack with data, **When** the new
   `sesh up` runs, **Then** every service is recreated on its new
   networks, the old shared network no longer exists, and all data is
   still there.
2. **Given** the upgraded stack, **When** `sesh up` runs again, **Then**
   nothing changes.

---

### User Story 4 - Development and evals still need no internet (Priority: P2)

In stub mode, the stub model server still plays the model for the
router, the fake Google, search, weather and maps for the gateway, and
the fake feed publisher for the feed service, and the eval project
still runs, all with no internet, as CI requires.

**Why this priority**: CI is the only real test of the stack, and it
runs with containers' internet blocked.

**Independent Test**: The existing stub-mode stack tests and the eval
suite pass with containers' internet blocked.

**Acceptance Scenarios**:

1. **Given** stub mode, **When** the router, the gateway and the feed
   service use the stub, **Then** each reaches it, over a network that
   exists only in stub mode.
2. **Given** the household (non-stub) configuration, **When** its
   networks are listed, **Then** that stub-only network doesn't exist.

---

### User Story 5 - The operator can see the layout is intact (Priority: P3)

The doctor tells the operator when the network layout has drifted: a
catch-all network has come back, the database is attached to a
network other than its per-service ones, or a service is attached to a
network its row doesn't name.

**Why this priority**: Layout drift (a hand-edited override file, an
old container) silently undoes isolation.

**Independent Test**: With the layout intact the doctor's check passes;
attaching the chat UI to the secret store's network makes it fail and
name the service and the network.

**Acceptance Scenarios**:

1. **Given** the intended layout, **When** the doctor runs, **Then** the
   network check passes.
2. **Given** a service attached to a network outside its row, **When**
   the doctor runs, **Then** the check fails and names the service and
   the network.

### Reachability table

What each service may open a connection to. Anything not listed is
refused. "Edge" means the service's own single-member network that
carries its published loopback port and its outbound access, if any.

| Service | May reach | Reached by |
|---|---|---|
| Database | — | each storing service, on its own database network |
| Model router | its database; the inference server (via its edge) | the chat UI; the operator on its loopback port |
| Chat UI | the model router; its database; its cache | the household on its loopback port |
| Gateway | its database; the secret store; the approval channel; search, documents and feeds (existing networks); executors (existing networks); Google, weather and maps (via its edge) | agents and the operator on its loopback ports |
| Secret store | its database; its cache | the gateway; the operator on its loopback port |
| Approval channel | upstream push relays (via its edge) | the gateway; phones on its loopback port (via the tailnet) |
| Document service | its database; its cache | the gateway (existing documents network) |
| Feed service | unchanged from feature 006 | unchanged |
| Search, caches, executors | unchanged | unchanged |

### Edge Cases

- **A service with two non-internal networks** would let the system
  pick either as its default route. Each service has at most one
  non-internal network (its edge), so its default route, and the
  gateway's listener address, is unambiguous.
- **The gateway's listener** binds the address of the interface that
  holds its default route. With `default` gone, that is its edge
  network; the agent and approval listeners must still never bind on an
  executor network or a resource network.
- **The model server on the host** (reached through the host's gateway
  address) must stay reachable from the model router's edge network.
- **An override file** that still names the old shared network fails
  loudly at `sesh up` rather than recreating it.
- **The eval project** has its own networks and is unchanged.
- **Optional components** (the offline encyclopedia, per-agent
  executors) keep their existing networks.
- **A network left from the old layout** (the old shared network) is
  removed by `sesh up` once no container uses it.
- **Network address ranges are chosen by the container engine**, not
  fixed by sesh, so they can differ between installs and change if a
  network is recreated. `sesh up` reads the actual ranges every time and
  rewrites the database's login rules to match.
- **If the ranges can't be read**, the database keeps only local access
  for administration and refuses every service login, and `sesh up`
  fails with a clear message: it fails closed, never open.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The stack MUST have no network shared by services beyond
  those its purpose names; the old shared network MUST NOT exist after
  `sesh up`.
- **FR-002**: The database MUST be attached only to per-service
  database networks, one for each service that stores data in it
  (model router, chat UI, gateway, secret store, document service, feed
  service), each shared by the database and that one service.
- **FR-003**: The chat UI and the model router MUST share a network that
  no other service is on.
- **FR-004**: The gateway and the secret store MUST share a network that
  no other service is on.
- **FR-005**: The gateway and the approval channel MUST share a network
  that no other service is on.
- **FR-006**: Each service that publishes a loopback port or needs
  outbound access (chat UI, model router, gateway, secret store,
  approval channel) MUST have its own non-internal edge network with no
  other member; every other network a service joins MUST be internal.
- **FR-007**: No service MUST be attached to more than one non-internal
  network.
- **FR-008**: The existing networks for search, documents, feeds, the
  per-service caches and the per-agent executors MUST keep their
  members and settings.
- **FR-009**: The gateway's agent and approval listeners MUST bind
  loopback plus the address of its edge network, and MUST NOT bind an
  address on any executor, resource, database or pair network.
- **FR-010**: Published ports MUST stay bound to the server's loopback
  address only, as today.
- **FR-011**: In stub mode, the stub MUST be reachable by the model
  router, the gateway and the feed service over a network that exists
  only in stub mode; the household configuration MUST NOT contain it.
- **FR-012**: `sesh up` on a pre-008 install MUST move every service to
  its new networks and remove the old shared network, without changing
  any stored data; running it again MUST change nothing.
- **FR-013**: The doctor MUST check the layout and fail, naming the
  service and the network, when a catch-all network exists, the
  database is on any non-database network, or a service is on a network
  its row doesn't name.
- **FR-014**: A stack test MUST check the reachability table in both
  directions from inside each service's network namespace: every
  allowed path connects (positive controls) and every other
  service-to-port pair fails.
- **FR-015**: Every existing stack test MUST pass with the new layout.
- **FR-016**: The decision MUST be recorded in an ADR (it changes what
  talks to what and the gateway's listener rule), and the architecture
  document MUST describe the new layout in the same change.
- **FR-017**: The roadmap MUST be renumbered: 008 network isolation,
  009 read-only Google Docs, 010 memory, 011 one agent, 012 browser,
  013 household.
- **FR-018**: Each service's database login MUST be accepted only from
  the address range of that service's own database network; every other
  source MUST be refused, including other networks, other services and
  the database's own default rules.
- **FR-019**: The database's administrative login MUST be accepted only
  through the database container's local connection, never over the
  network.
- **FR-020**: The login rules MUST be derived on every `sesh up` from the
  address ranges the container engine actually assigned, with no fixed
  ranges in sesh's configuration, and applied without restarting the
  database.

### Key Entities

- **Database network**: an internal network shared by the database and
  exactly one storing service.
- **Pair network**: an internal network shared by exactly two services
  for one named link (chat UI ↔ model router, gateway ↔ secret store,
  gateway ↔ approval channel).
- **Edge network**: a non-internal network with exactly one member,
  carrying that service's published loopback port and its outbound
  access.
- **Stub network**: an internal network present only in stub mode,
  joining the stub with the services that use its fakes.
- **Reachability table**: the allowed (service → destination) pairs;
  the source of truth for the stack test and the doctor check.
- **Database login rule**: a database login, the database it may open,
  and the one address range it may connect from (its database network's
  current range).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the reachability table's allowed paths connect,
  and 0 of the disallowed service-to-port pairs connect, in the stack
  test.
- **SC-002**: All existing stack tests (seven before this feature) pass
  on the new layout, in CI with containers' internet blocked.
- **SC-003**: After an upgrade with `sesh up`, 100% of the data created
  beforehand in each service is still present, and a second `sesh up`
  makes no changes.
- **SC-004**: The database is reachable from exactly the six storing
  services and from nothing else in the stack.
- **SC-005**: No network other than an edge network has a member that
  isn't named for it, and no edge network has more than one member.
- **SC-006**: The doctor's layout check fails within one run when any
  service is attached to a network outside its row.
- **SC-007**: 100% of service database logins are refused from every
  source other than their own database network, and the administrative
  login is refused over the network from every container.

## Assumptions

- **Per-login source restrictions use the engine's own ranges**: the
  container engine chooses each network's address range; sesh never
  fixes one (that would risk clashing with the household's LAN or VPN).
  The database's login rules are regenerated from the actual ranges on
  every `sesh up`.
- **Outbound access through an edge network isn't narrowed**: an edge
  network gives its service general outbound access, including services
  that don't need it (the chat UI, the secret store). Compose can't
  express egress filtering; the chat UI stays in its offline mode, and
  host firewall rules are a possible follow-up.
- **The approval channel keeps an edge network**: it is needed for its
  published port, and also lets it use an upstream push relay if the
  household's phones subscribe that way.
- **The offline encyclopedia and the executors** keep their existing
  networks.
- **The eval project** keeps its own separate networks and isn't
  changed.
- **Containers are recreated, not modified in place**: moving a
  container between networks restarts it; volumes, and so all data, are
  untouched.
- **Network names are an implementation detail**: the names in the
  feature description (`litellm-db`, `models`, `secrets`, `notify`, …)
  are the intended ones and may be adjusted in the plan.
