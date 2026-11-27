# sesh — Solutions Architecture

This document is sesh's design source (ADR 0001). Changes to it go
through pull requests like any other change. If one adds, removes or
renames a component, or changes what talks to what, update the diagram in
the same change (see [README.md](README.md) in this folder).

## Overview

sesh is a local-first agent platform for one household: AI agents that
act on each user's behalf through a single trusted gateway, so no agent
ever holds a secret. It is assembled mostly from existing open-source
parts; the custom work is the gateway and a few small connectors.

sesh runs in two modes on the same stack:

- **Operator mode:** the admin sets up and maintains sesh (adding users,
  rotating secrets, updates) with Claude Code skills that drive a `sesh`
  admin CLI. No agent can reach this tooling.
- **Assistant mode:** always-on agents for each household user, plus a
  shared Household agent. People reach them through Open WebUI or
  Telegram, for Muse-style tasks: email, calendar, browsing, documents,
  errands.

**Goals**

- Each user gets an isolated agent. Shared accounts are used only by the
  Household agent.
- Agents can use a browser and log in to sites without ever seeing
  credentials.
- Inference engines can be swapped without touching clients.
- It builds now on an M5 Max 128GB MacBook Pro, and runs long-term on a
  Mac mini or Studio with under 128GB.

**Non-goals**

- Cloud models or hosted agent platforms.
- Moving money, trading, or sending as a user without approval.
- iMessage, home automation, GitHub integration and a dev-mode coding
  harness (deferred).

## Requirements and design principles

Every design choice below traces to one of these eight rules. When an
implementation choice conflicts with one, the rule wins.

1. **One way out.** Agents reach accounts, data, memory, documents and
   the browser only through the Code Mode gateway. Agents have no
   built-in terminal, file or web tools.
2. **Agents never see secrets.** Credentials live in Infisical and are
   used by the gateway alone. Logins are filled into the browser by the
   gateway, never typed by the model.
3. **Agents are untrusted.** Any agent can be manipulated by an email, a
   web page or another agent. Isolation and approvals are enforced in the
   control plane, never in prompts.
4. **Per-user isolation.** Each user's agent can reach only that user's
   accounts, memory and documents. Shared accounts belong to the
   Household agent alone.
5. **Side effects need approval.** Purchases, sends, deletes and other
   external changes pause for a human yes, delivered through ntfy by the
   gateway.
6. **Local-first.** All inference and data stay on the home server.
   These exceptions are named:
   - Google APIs, via OAuth;
   - search engines, behind SearXNG;
   - the content-free iOS wake-up through ntfy.sh.
7. **Adopt before build.** Use existing open-source components. Build
   only the gateway and the connectors no project provides.
8. **Swappable inference.** Clients call model aliases (`agent-main`), so
   the engine behind them can change without client changes.

**Constraints**

- Bare metal is for the inference server only, when it needs the GPU
  (for example oMLX on Metal). Everything else runs in containers or sbx
  microVMs.
- The always-on server targets 64GB of unified memory. Peak use is
  planned at about 45–50GB.

## Architecture overview

sesh is organized by trust. People reach untrusted agents through
channels, and agents reach every resource only through the trusted
control plane.

![sesh architecture: six zones by trust](../architecture.svg)

Requests flow top to bottom. The dashed accent path is the control
plane's own channel: approvals and one-time links go from the gateway to
people through ntfy, never through an agent.

## Component catalog

sesh has 20 components across six zones. Three are custom builds (marked
**build**); the rest are adopted and configured.

| Component | Zone | Role | Technology | Runs as | Key configuration |
| --- | --- | --- | --- | --- | --- |
| Open WebUI | Channels | Accounts and chat front end for household agents | Open WebUI | Container | Built-in Memory off; raw models hidden from users; admin chat access off; task model = `agent-main` |
| Telegram | Channels | Convenience chat channel | Telegram bot per agent | Hermes gateway | Not end-to-end; no sensitive content |
| ntfy | Channels | Approvals, one-time links and alerts, from the gateway only | ntfy (self-hosted) | Container | Auth required, deny by default; topic per user; action buttons; iOS upstream = ntfy.sh |
| Hermes | Agents | Agent runtime for each user and Household | Hermes Agent | Container per profile | Built-in terminal, file and web tools off; gateway is the only MCP tool; skills in git |
| Code Mode gateway | Control plane | Single path to every resource; scoping, approvals, audit | `@cloudflare/codemode` runtime in per-agent workerd executors; gateway in Rust | Container | Token per agent; MCP over Streamable HTTP; scripts run in an untrusted workerd executor container per agent, with no network or secrets; see Build vs adopt (**build**) |
| Infisical | Control plane | Secrets and site logins, per agent | Infisical (self-hosted) | Container | Project + machine identity per agent; read only by the gateway |
| Audit log | Control plane | Append-only record of every gateway action | Postgres table | Shared Postgres | Insert-only grants; 90-day full retention; agents cannot read |
| Gmail + Calendar + Contacts | Resources | Personal and household Google data | Google APIs, called by the gateway (ADR 0008) | Gateway connectors | One Google Cloud project, app in production; read-only + compose scopes; send behind approval |
| SearXNG | Resources | Web search | SearXNG | Container | JSON output; reachable from the gateway only |
| Kiwix | Resources | Offline Wikipedia | kiwix-serve + ZIM files | Container | Text-only English Wikipedia (~50GB); quarterly refresh |
| Weather + maps | Resources | Forecasts, geocoding, places | Open-Meteo, OpenStreetMap | Gateway connector | No keys; home location fixed in the gateway |
| Miniflux | Resources | Feeds (RSS, Atom) the household follows | Miniflux (ADR 0009) | Container | User per owner, managed by the gateway; reachable from the gateway only (`feeds` network); its own egress network to publishers and its own route to Postgres, never `default`; agent adds need approval; web UI not published |
| Paperless-ngx | Resources | Household documents: OCR, search | Paperless-ngx (+ Paperless-AI if needed) | Container | User per agent scope; search, read and upload via the gateway; a mail rule imports a Gmail "To file" label; embeddings on CPU |
| Browser | Resources | Logged-in browsing, live view, `read(url)` | Chromium (Playwright build), headful | sbx microVM per session | Profile per agent; fresh context per read; private addresses blocked (**build**: `login(site)`, `read(url)`) |
| Mem0 | Resources | Long-term memory per user + household | Mem0 OSS, pgvector, BM25 | Container | `infer=False` at first; CPU embeddings; `user_id` set by the gateway |
| Postgres + Redis | Resources | Storage for the adopted services and the audit log | Postgres 16 + pgvector; Redis, one instance per service | Container | One Postgres server; each service logs in as its own role, owning only its database, never the superuser; a Redis per service (Open WebUI, Infisical, Paperless), each reachable only from it (ADR 0010). Backs Mem0, Open WebUI, LiteLLM, Paperless, Miniflux, Infisical and the audit log |
| Inference server | Inference | Serves models behind aliases (OpenAI-compatible API) | Any OpenAI-compatible server (for example oMLX, Ollama, llama.cpp, vLLM) | Bare metal | Alias `agent-main` |
| embed | Inference | Embeddings for Mem0 and Paperless | Small embedding model | In-container CPU | Same model for both; never swapped without re-embedding |
| LiteLLM | Inference | The alias router every client calls (ADR 0005) | LiteLLM | Container | Always present; clients get alias-only keys; later: failover, a second engine |

The third custom build is the Hermes memory provider plugin, which
routes Hermes memory through the gateway (see Memory, knowledge and
skills).

## Security model

The trust boundary is the control plane. Agents are assumed compromised
at any moment, so every protection lives in the gateway, Infisical, ntfy
and the audit log.

**Agent tokens and scoping**

- Each agent (one per user, plus Household) has one gateway token,
  injected into its container or sandbox at start.
- The gateway maps each token to an allowlist of connectors and to that
  user's identities: Infisical project, Google tokens, Paperless user,
  Mem0 `user_id`.
- The model never passes a user or scope. The gateway derives it from
  the token.

**Code Mode sandbox**

- Model-written code runs in an isolated executor with no environment,
  filesystem, process or network access.
- The only callable surface is the connector globals for that token.
- The executor is untrusted and runs one per agent. The gateway re-checks
  scope on every call, and runs a gated action only with a one-time
  approval, whatever the executor reports.

**Secrets**

- Infisical holds OAuth refresh tokens, API keys and site logins, in one
  project per agent.
- Only the gateway's machine identities read them. Values are used in
  memory and never logged or returned.
- New logins arrive through `request_credential(site)`:
  1. The gateway sends a one-time form link via ntfy.
  2. The user enters the credential there.
  3. The gateway writes it to Infisical.
  4. The agent learns only that it is stored.

**Browser logins: `login(site)`**

- The gateway reads the browser's real URL, and refuses unless it
  matches the domain saved with the credential.
- It fills fields over CDP and generates TOTP codes itself. The agent
  receives only success or failure.
- Agent browser tools can't run arbitrary JavaScript or read input
  values, and snapshots mask password fields.

**Approvals**

- Purchases, sends, deletes, settings changes and first logins to a new
  site pause the execution.
- The gateway writes the approval text itself and sends it via ntfy,
  with a one-time link to approve or deny (ADR 0007).
- Household actions notify both users. Personal actions notify only the
  owner.

**Untrusted content**

- Email, web pages, documents, other agents' requests and GitHub-style
  content are data, never instructions.
- The gateway labels returned content as untrusted.
- `read(url)` blocks private, LAN and Tailscale addresses, and limits
  size and time.
- Feeds are fetched by Miniflux on its own networks, never by the
  gateway (ADR 0009); feed text comes back as quoted, capped data.

**Audit log**

- Every call records:
  - agent, user, connector and method;
  - a redacted argument summary;
  - approval decisions;
  - the result, or the refusal reason.
- The table is insert-only, and agents can't read it.
- Repeated refusals or bursts trigger an ntfy alert.

**Storage isolation**

- Every adopted service is untrusted. Each logs in to Postgres as its
  own role, which owns only that service's database and has no
  administrative rights; only that role may connect to it. The
  superuser is for sesh's own admin tasks (`sesh up`, `sesh doctor`),
  over the Postgres container's local socket (ADR 0010).
- So a compromised service can't read or change the audit log, the
  secret store's database or another service's data. `sesh doctor`
  fails if a service is connected as the superuser.
- Each service that uses Redis has its own instance, on an internal
  network shared only with it.

**Network**

- All services listen on the Docker network or localhost. People reach
  Open WebUI and ntfy over Tailscale only.
- Containers and sandboxes may reach the inference server and the
  gateway. Only the gateway and named resources reach the internet.

## Users, agents and household model

Each user gets one personal agent, and the household gets one shared
agent. Adding a user means adding one profile, one Infisical project, one
Mem0 scope and one Paperless user, all created by one operator command
(see Build vs adopt, item 4).

| Agent | Who uses it | Accounts it can use | Memory scope | Channels |
| --- | --- | --- | --- | --- |
| `<user>` (one per user) | That user | That user's Google, site logins, Paperless docs | `<user>` | Open WebUI, Telegram bot |
| Household | All users | Shared accounts only (household Google account, shared logins) | `household` (others read it) | Open WebUI (visible to all), Telegram bot |

**Rules**

- A personal agent never uses another user's accounts, memory or
  documents.
- Shared accounts are separate items in the Household Infisical project,
  never copies of personal logins.
- Open WebUI model access: each user sees only their own agent and
  Household. Raw models are hidden.

**Agent-to-agent: `ask_household(task)`**

- A user can say "tell the household agent to do X", and the personal
  agent calls `ask_household`.
- The gateway allows personal → Household only, never personal →
  personal. It logs the request and forwards it over Hermes A2A.
- The Household agent runs the task with its own tools, and side effects
  still need approval. The reply returns to the chat that asked.
- Hermes's own outbound A2A tools stay off; the gateway is the only
  route.

## Memory, knowledge and skills

Each store has one job:
- Mem0 is the single source of truth for facts about people.
- Paperless holds original documents.
- Kiwix holds reference knowledge.
- Hermes keeps only skills and the agent's working notes.

| Store | Holds | Scoping | Reached by |
| --- | --- | --- | --- |
| Mem0 | Facts and preferences about users and the household | `user_id` per user + `household`, set by the gateway | Gateway `recall` / `remember`; Hermes memory provider plugin → gateway |
| Paperless-ngx | Original documents, OCR text, semantic index | Paperless user per agent scope | Gateway (search, read, upload) |
| Kiwix | Offline Wikipedia | None (public) | Gateway |
| Hermes `USER.md` | Basics per user (name, time zone, reply style) | Per profile | Hermes only; maintained by people, read-only to the agent |
| Hermes `MEMORY.md` | Agent working notes, no facts about people | Per profile | Hermes only; mirrored to Mem0 by the plugin |
| Hermes skills | Self-learned procedures | Per profile; household skills read-only to all | Hermes only |

**Mem0 configuration**

- Open-source Mem0 2.x, with `mem0ai[nlp]` for hybrid semantic + BM25 +
  entity retrieval.
- `infer=False` at launch: agents write clean facts through `remember`.
  Turn on extraction later, with `agent-main`, if recall gaps appear.
- Mem0 is add-only, so superseded facts remain. Schedule a cleanup job,
  and give each user a page to view and edit their own memories.

**Recall across stores**

- One Code Mode script searches the user's Mem0 scope, the user's
  Paperless documents and Kiwix, then merges the results with source
  labels.
- No combined search service is built.

**Skills**

- Each profile's skills folder is a git repository. A scheduled job
  commits changes hourly, and the repositories are backed up with the
  server (ADR 0003).
- Household skills live in a separate repository mounted read-only.
  Promotion is by human copy or an approved pull request only.
- Review the commit log for skills that add new recipients, addresses or
  actions.

## Deployment and runtime

Only the inference server runs on bare metal, when it needs the GPU.
Long-lived services run in Docker from one Compose file, and agent work
that touches untrusted content runs in sbx microVMs.

| Placement | Components | Why |
| --- | --- | --- |
| Bare metal (macOS) | Inference server, when it needs the GPU (for example oMLX) | Needs the Metal GPU |
| Docker (Compose) | Open WebUI, Hermes (container per profile), gateway, workerd executor per agent (no network), Infisical, ntfy, Mem0, Paperless, Miniflux, SearXNG, Kiwix, LiteLLM, Postgres, Redis (one per service) | Long-lived services; one Compose file moves to the Mac mini unchanged |
| sbx microVM | Browser sessions | VM boundary for code execution and untrusted web content |

**Memory budget on a 64GB Mac mini (estimates)**

| Item | Estimate |
| --- | --- |
| macOS | 5–6 GB |
| `agent-main` (~30B MoE, 4-bit) | 18–20 GB |
| KV cache, a few concurrent chats | 4–8 GB |
| Inference server overhead | ~1 GB |
| Docker Desktop VM (capped) | 10 GB |
| Browser sessions in sbx, when active | 1–2 GB each |
| **Idle total** | **~38–45 GB** |
| **Peak, two agents browsing** | **~45–50 GB** |

**Environments**

- **Laptop (M5 Max 128GB):** build and test everything.
- **Mac mini or Studio (64GB+):** runs assistant mode only, with
  `agent-main` only.

**Operations**

- Cap Docker Desktop memory at about 10GB. Swap to OrbStack later to
  recover roughly 5GB.
- The macOS GPU wired limit defaults to about 75% of RAM. Raise
  `iogpu.wired_limit_mb` only if a larger model needs it.
- Back up:
  - Postgres (memory, document metadata, secret metadata, the audit
    log);
  - the skills repositories;
  - Paperless media.
- Update monthly, in a set window. Health checks alert via ntfy.
- Schedule large document imports overnight, since they share the GPU
  with chat.

## Build vs adopt

sesh builds one service, the gateway, plus a Hermes memory plugin and a
small browser controller. Everything else is configuration.

**1. `sesh-gateway` (Rust)**

- An MCP server (Streamable HTTP). Scripts run on `@cloudflare/codemode`'s
  runtime in per-agent workerd executors (see Code execution below).
- Auth: a bearer token per agent maps to the agent record
  `{agent_id, user_id, connectors[], infisical_project, approvers[]}`.
- Connectors, each a typed namespace in the sandbox:

| Connector | Methods | Side effects → approval |
| --- | --- | --- |
| `mail`, `calendar`, `contacts` (Google, ADR 0008) | `mail.search`, `mail.read`, `mail.draft`, `mail.send`, `calendar.list`, `calendar.create`, `contacts.search` | send, create |
| `memory` | `recall(query, {includeHousehold})`, `remember(fact)`, `forget(id)` | writes to `household` |
| `docs` | `docs.search`, `docs.get`, `docs.upload(ref, {tags, title})` | uploads to household documents notify both users |
| `search` | `searxng.search`, `kiwix.search`, `kiwix.article` | none |
| `geo` | `weather.forecast`, `maps.geocode`, `maps.nearby` | none |
| `feeds` (ADR 0009) | `feeds.list`, `feeds.latest({feed, since?, limit?})`, `feeds.add({url, name?})` | add (a new outbound address) |
| `browser` | `open`, `snapshot`, `click`, `type`, `login(site)`, `read(url)`, `liveViewLink()`, `download()` → download ID | purchases, form submits, first login per site |
| `household` | `ask_household(task)` | as the Household agent decides |
| `secrets` | `request_credential(site)` | the user fills the form |

**Document ingestion**

- **People add documents directly**, with no agent involved: the
  Paperless web app, a watched folder, phone scanning apps, and a
  Paperless mail rule that imports attachments from a Gmail "To file"
  label.
- **Agents file documents** with `docs.upload(ref, {tags, title})`.
  - `ref` is an ID the gateway already holds: a Gmail attachment, a
    browser download, or a file the user sent in Open WebUI or Telegram.
  - The gateway fetches the bytes itself, so file contents never enter
    model context.
- **Ownership** comes from the agent's token: personal agents file to
  their user, and Household files to shared documents.
- **Downloads:** browser `download()` keeps the file inside the browser
  sandbox and returns only a download ID.
- **Limits:**
  - PDF, image and Office types only, with a size cap;
  - every upload is audited;
  - personal uploads need no approval, and household uploads notify both
    users via ntfy.

**Other gateway parts**

- **Approval engine:** a gated call stops and the person gets an ntfy
  message with a one-time approve/deny link. Once they approve, the
  identical call runs once (ADR 0007).
- **Credential form:** a one-time HTML page, Tailscale-only, that writes
  to Infisical.
- **Audit writer:** an insert-only Postgres table.
- **Egress guard:** blocks private, LAN, Tailscale and localhost targets
  for `read(url)` and browser navigation.
- **Snippet store (later):** named scripts with search, if regenerating
  scripts becomes common.

**Code execution: workerd**

- **Isolation:** model-written scripts run in
  [workerd](https://github.com/cloudflare/workerd), Cloudflare's
  open-source Workers runtime, with a fresh V8 isolate for each run.
- **Deadlines:** local workerd enforces no CPU limits, so the gateway
  enforces a deadline per run and restarts an executor that overruns.
- **The executor:** workerd runs as its own container (`sesh-executor`),
  one per agent, with no network and no secrets. It is untrusted.
  - It runs the Code Mode runtime: the call log, approvals by replay, and
    search.
  - Its connector stubs send every call to the gateway, where scope,
    approvals, Infisical access and the audit are applied, whatever the
    executor reports.
- **Not vm2:** vm2-based executors are deprecated after repeated sandbox
  escapes.
- **Durable runtime:** Code Mode's durable runtime (replay, snippets)
  runs under local workerd and is adopted inside the executor. The
  gateway still enforces approvals itself.

**2. `hermes-memory-sesh` (Python, inside the untrusted agent runtime)**

- A Hermes memory provider plugin that calls the gateway's `memory`
  connector with the profile's token.
- It gives automatic recall and mirrored writes, while keeping the
  gateway as the only path.

**3. `sesh-browser` (TypeScript, Playwright, inside the sbx VM)**

- A controller inside the sbx browser VM:
  - a persistent context per agent, and a fresh context per `read(url)`;
  - CDP fill for `login(site)`;
  - a noVNC live view;
  - Readability extraction to Markdown.
- Evaluate adopting Auto Browser, easi.run or Nod's browser before
  building.

**Adopted as-is (configure only):** Open WebUI, Hermes, an
OpenAI-compatible inference server, LiteLLM, Infisical, ntfy, Mem0,
Paperless-ngx, SearXNG, Kiwix, Postgres, Redis, sbx.

**4. `sesh` admin CLI + Claude Code operator skills**

- It runs on the server as the admin, outside every agent's reach. No
  agent token can call it.
- `sesh user add <name>` provisions a user end to end:
  - an Open WebUI account and model access;
  - a Hermes profile container, from a template;
  - a gateway token and agent record;
  - an Infisical project and machine identity;
  - a Mem0 scope and a Paperless user;
  - an ntfy user and topic;
  - a Telegram bot binding;
  - a skills repository.

  It finishes by sending the user their Google consent link via ntfy.
- `sesh user remove <name>` revokes the gateway token, the Infisical
  identity and the Google grants, then archives that user's memory,
  documents and skills.
- Maintenance commands: `sesh doctor` (health checks), `sesh backup`,
  `sesh secrets rotate`, `sesh update`.
- Claude Code skills in `skills/operator/` wrap these commands with
  checklists: add user, remove user, rotate secrets, restore a backup.
  Destructive steps need explicit confirmation.
- Every operator action writes to the audit log as `actor: operator`.

## Implementation plan

Build in thirteen phases, each ending in a working end-to-end path. Don't
start a phase until the previous one's acceptance checks pass. Each phase
is a feature under `specs/`.

1. **Foundation:** an OpenAI-compatible inference server behind the
   `agent-main` alias; Compose with Postgres, Redis and Open WebUI;
   Tailscale access.
   - Accept: Open WebUI over Tailscale chats with `agent-main`; raw
     models are hidden from a non-admin test user.
2. **Gateway core:** `sesh-gateway` with token auth, the Code Mode
   executor, the audit writer, and the `search` (SearXNG, Kiwix) and
   `geo` connectors.
   - Accept:
     - a script calling two connectors in one execution works;
     - the sandbox can't read env, files or network (tests);
     - every call appears in the audit table;
     - an unknown token is refused.
3. **Model evals:** `sesh eval`.
   - Scenarios are data: a request, the connectors on offer, and what
     correct looks like.
   - They run through the gateway against fake connectors with the real
     schemas.
   - Rules grade them: calls made, arguments, approval pauses, and
     injected content treated as data.
   - Each run gives a pass rate per alias, saved per model, and can run
     against the operator's real inference server.
   - Accept: the suite runs against any configured alias and reports a
     pass rate; a model that can't call tools scores 0; results are
     comparable across models.
4. **Secrets and approvals:** Infisical, read only by the gateway; ntfy
   with approve/deny; one-time credential forms.
   - Accept:
     - an approval-gated test method runs only after ntfy approval,
       exactly once, with the approved arguments;
     - a credential entered through the one-time form reaches its
       connector;
     - no secret value appears in model context, logs or audit rows
       (grep tests).
5. **Google and Paperless:** the `mail`, `calendar` and `contacts` connectors with an OAuth
   consent page; Paperless; per-user identity on agent records, so each
   agent reaches only its user's accounts and documents.
   - Accept:
     - Gmail search works;
     - `mail.send` pauses, and runs only after ntfy approval;
     - an email attachment filed with `docs.upload` lands in that
       user's documents, without its bytes entering model context;
     - user A's agent can't reach user B's mail or documents (negative
       tests).
6. **RSS feeds:** Miniflux behind the gateway's `feeds` connector, with
   a Miniflux user per owner; feeds added by people with `sesh feed
   add`, or by an agent with approval (ADR 0009).
   - Accept:
     - each agent reads its owner's and the Household's feeds, never
       another user's (negative tests);
     - Miniflux can't reach the gateway or Postgres on `default`, and no
       executor can reach Miniflux;
     - an agent's `feeds.add` runs only after approval.
7. **Service isolation:** a Postgres login per adopted service, owning
   only its database, and a Redis per service on its own network;
   existing installs handed over in place by `sesh up` (ADR 0010).
   - Accept:
     - each service's login reaches only its own database (negative
       tests), and no service reaches another's Redis;
     - an install from before this phase keeps its data and works after
       `sesh up`;
     - `sesh doctor` fails when a service uses the superuser.
8. **Network isolation:** no shared network. Each service is on a
   database network to Postgres, the pair networks for its links, and
   one edge network of its own for its published port and outbound
   access; each database login is accepted only from its own network's
   address range, read from Docker on every `sesh up` (ADR 0011).
   - Accept:
     - from inside each service, only the paths in the layout table
       connect, and everything else is refused (negative tests);
     - a database login is refused from any other network, and the
       superuser over any network;
     - `sesh doctor` fails when a service is on a network outside its
       row.
9. **Google Docs (read-only):** search and read the owner's Google Docs
   through their existing Google grant, with a read-only scope.
   - Accept: an agent reads its owner's documents, never another
     user's; nothing can be written.
10. **Memory:** Mem0 (CPU embeddings, `infer=False`) behind the gateway's
   `memory` connector, scoped per user; cross-store recall with
   documents. Mem0 gets its own login; the migration creates the
   `vector` extension, which needs the superuser.
   - Accept: a fact saved in one session is recalled in a later one;
     user A's agent can't recall user B's facts or documents (negative
     tests).
11. **One agent end to end:** one Hermes profile in a container, with
   built-in tools off, the gateway as its only MCP server, and a memory
   provider plugin backed by the gateway; Open WebUI connected to it.
   - Accept: chatting in Open WebUI, the agent answers a weather and a
     search question, and uses mail, documents and memory via the
     gateway; attempts to use terminal or file tools fail.
12. **Browser:** the sbx browser VM, a persistent profile per agent,
   `read(url)`, `login(site)`, and a live-view link via ntfy.
   - Accept: `read(url)` returns Markdown and refuses a LAN address;
     `login(site)` refuses on a lookalike domain; the agent can't read a
     filled password field.
13. **Multi-user and household:** profiles for each user and Household,
   Telegram bots, `ask_household`, skills repositories with hourly
   commits, backups, health alerts, and the operator CLI with its Claude
   Code skills.
   - Accept:
     - "tell the household agent to add milk" works from either user;
     - personal → personal requests are refused;
     - restore from backup succeeds on a clean machine;
     - `sesh user add` provisions a test user end to end, and `sesh user
       remove` revokes every credential.

**Working notes for Claude Code**

- Treat the principles as hard constraints, and write negative tests for
  each isolation rule. The binding version is
  `.specify/memory/constitution.md`.
- Never put real credentials in the repository (ADR 0003).
- Prefer configuring adopted projects over new code, and record every
  deviation in `docs/decisions/`.

## Risks and open questions

The largest risks are unverified third-party behavior and the quality of
a local model as a tool caller. Each item below has a check to run
before or during its phase.

| Item | Type | Check or mitigation | Phase |
| --- | --- | --- | --- |
| `agent-main` tool-calling and memory quality on a ~30B local model | Risk | Benchmark candidate models on real tasks (`sesh eval`); keep prompts and tool docs short | 3 |
| Code Mode durable runtime (snippets, replay) outside Cloudflare | Resolved | Verified under local workerd; it enforces no CPU limits, so one executor per agent and gateway-enforced deadlines | 2 |
| Hermes skill writing still works with file and terminal tools off | Open question | Verify the skills tool writes only inside the profile's skills folder | 10 |
| Hermes memory provider interface fits a gateway-backed plugin | Open question | Read the provider ABC; prototype against the `memory` connector | 10 |
| Browser server choice (Auto Browser, easi.run, Nod, or build) | Open question | Vet code and isolation; confirm CDP fill and field-read blocking | 11 |
| Paperless built-in AI covers semantic search | Open question | Check the current release; add Paperless-AI only if needed | 5 |
| Feed addresses used for server-side request forgery or exfiltration | Risk | Only Miniflux fetches, off `default` on its own networks; agent adds need approval (ADR 0009) | 6 |
| A compromised adopted service reads or changes other services' data or the audit log | Risk | A Postgres login per service owning only its database; a Redis per service; doctor's `db-isolation` (ADR 0010) | 7 |
| A service release that needs a superuser-only database extension | Constraint | The migration creates it as the superuser; none needed so far (`vector` will be, for memory) | 7, 9 |
| Prompt injection via email, pages, documents or other agents | Risk | Untrusted labels, approvals, domain-pinned login, egress guard, audit alerts | 4–12 |
| Mem0 add-only accumulates stale facts | Risk | Scheduled cleanup; per-user memory editing page | 9 |
| sbx requires a Docker sign-in (cloud dependency) | Risk | Review login and telemetry terms; fall back to containers if unacceptable | 11 |
| Telegram content passes Telegram servers | Risk | Convenience only; sensitive work in Open WebUI | 12 |
| Google OAuth app in production shows an unverified warning | Accepted | One-time click-through per user | 5 |
| Operator can read other users' chats, sessions and logs | Policy | Turn off Open WebUI admin chat access; agree a household policy | 1 |
| An embedding model change requires re-embedding all memories and documents | Constraint | Pick once; document the re-index procedure | 9 |

## References

- [Cloudflare Code Mode: how it works](https://developers.cloudflare.com/agents/tools/codemode/how-it-works/)
- [Cloudflare Code Mode: durable runtime](https://developers.cloudflare.com/agents/tools/codemode/durable-runtime/)
- [workerd](https://github.com/cloudflare/workerd)
- [oMLX](https://github.com/jundot/omlx)
- [Open WebUI: MCP](https://docs.openwebui.com/features/mcp)
- [Open WebUI: Memory](https://docs.openwebui.com/features/chat-conversations/memory)
- [Hermes Agent: Open WebUI integration](https://hermes-agent.nousresearch.com/docs/zh-Hans/user-guide/messaging/open-webui)
- [Hermes Agent: memory providers](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers)
- [Hermes Agent: A2A](https://hermes-agent.nousresearch.com/docs/user-guide/messaging/a2a)
- [Mem0 (PyPI)](https://pypi.org/project/mem0ai/2.0.0/)
- [Infisical](https://github.com/Infisical/infisical)
- [ntfy](https://github.com/binwiederhier/ntfy) · [ntfy docs](https://docs.ntfy.sh)
- [Paperless-ngx](https://github.com/paperless-ngx/paperless-ngx) · [Paperless-AI](https://github.com/clusterzx/paperless-ai)
- [Kiwix](https://kiwix.org) · [kiwix-tools](https://github.com/kiwix/kiwix-tools)
- [Docker Sandboxes (sbx)](https://docs.docker.com/ai/sandboxes/get-started/)
- [Google: transition from less secure apps to OAuth](https://knowledge.workspace.google.com/admin/sync/transition-from-less-secure-apps-to-oauth)
