# Research: Secrets and approvals

## R1. Who decides: the gateway, at `/call`, by abort and retry (ADR 0004)

- **Decision:** the executor stops pausing on approval-gated methods.
  Every call reaches `/call`, and the gateway decides:
  - It computes a fingerprint, the SHA-256 of agent, connector, method and
    the canonical JSON of the arguments.
  - **An approved, unused, unexpired approval for that fingerprint:** the
    gateway marks it used and runs the call once.
  - **A pending one:** it returns `approval_pending` with the same id, and
    sends no new notification.
  - **None:** it checks the rate limit, creates a request, notifies the
    person, and returns `approval_pending`.

  `execute` reports `status: "approval_pending"` with the request id and
  text telling the agent to run the same call again once it's approved.
- **Rationale:**
  - The executor is untrusted, so its pause or approval state can't
    matter (ADR 0004).
  - Fingerprint binding gives FR-005: the exact arguments, run once.
  - Retrying is the abort-and-replay that ADR 0004 anticipated, without
    needing Code Mode's durable resume.
- **Alternatives:**
  - *Resume the paused script after approval* (Code Mode's durable
    runtime). That keeps state in the untrusted executor, and approvals
    can take minutes, longer than an execution's deadline.
  - *The gateway re-runs the call itself after approval.* That would act
    without the agent, and the agent would never see the result.

## R2. Approval state in Postgres

- **Decision:** a new table, `approvals`, in the `gateway` database. The
  `sesh_gateway` role may INSERT, SELECT and UPDATE it, but not DELETE.
  The audit table stays insert-only.
- **Columns:**
  - `id`, `agent`, `connector`, `method`, `fingerprint`, `shown_text`;
  - `secret_sha256`: the approval secret's hash, never the secret;
  - `created_at`, `expires_at`;
  - `state`: pending, approved, denied, expired or used;
  - `decided_at`, `used_at`.
- **Rationale:** survives restarts (FR-008), and lets the audit link
  entries by request id.

## R3. Notifications: ntfy, self-hosted, with a "view" action

- **Decision:** an `ntfy` service in the stack (`binwiederhier/ntfy`,
  pinned), with these access settings:
  - `NTFY_AUTH_DEFAULT_ACCESS=deny-all`;
  - two users from `NTFY_AUTH_USERS`, with bcrypt hashes computed by
    `sesh init`: `sesh-gateway`, which may only write the topic, and
    `person`, which may only read it;
  - `NTFY_AUTH_ACCESS` granting just that.
- **The notification:** the gateway publishes JSON with:
  - a title ("Approve: <agent> wants to <method description>");
  - a body that the gateway writes, with the arguments quoted and
    truncated;
  - one **view** action, "Review", opening
    `<approvals base URL>/approve/<request id>#<secret>`.
- **The approval page:** served by the gateway. It shows the same
  gateway-written text, with Approve and Deny buttons that POST the
  secret.
- **Why a view action, not a one-tap http action:**
  - The person reads exactly what will happen before deciding (V).
  - The secret sits in the URL fragment, which isn't sent in requests
    or logged by the server, and is POSTed only on a tap.
  - SC-001 (under 30 s) still holds: tap the notification, then
    Approve.
- **Network:** ntfy sits on the stack's internal network, and the gateway
  reaches it there. The person's phone reaches ntfy, and the approval
  page, through `tailscale serve` (operator step). Neither is ever on an
  agent or executor network.
- **CI and the eval:** a real ntfy container. Tests read the topic with
  ntfy's poll API (`GET /<topic>/json?poll=1`, as `person`) and follow
  the link. That is the "stand-in" the spec allows, but it's the real
  thing, so no fake is needed.
- **Alternatives:** a bespoke fake notifier tests less of the real path.
  A one-tap http action skips reading the details.

## R4. A third gateway listener: approvals and forms

- **Decision:** port 7002 serves `/approve/<id>` (GET page, POST
  decision), `/form/<id>` (GET page, POST value) and `/admin/*`:
  - `/approve/<id>` and `/form/<id>` work without login, but need the
    per-request secret.
  - `/admin/*` needs `SESH_GATEWAY_ADMIN_TOKEN`, from `secrets.env`,
    passed to the gateway only.
  - Agent bearer tokens are refused on this listener (FR-007).
- **Binding:** like `:7000`, it binds to loopback and the default-route
  address only, never to executor networks. Compose publishes it on
  `127.0.0.1:7002` only.
- **Base URL:** `approvals_base_url` in `sesh.toml`, default
  `http://127.0.0.1:7002`. The operator sets it to their Tailscale serve
  URL for the phone.

## R5. Secret store: Infisical, bootstrapped by `sesh up`

- **Decision:** an `infisical` service, using the existing Postgres
  (database `infisical`) and Redis, with `ENCRYPTION_KEY`, `AUTH_SECRET`,
  `TELEMETRY_ENABLED=false` and `DISABLE_UPDATE_CHECK=true`.
- **On first `sesh up`, the CLI:**
  1. calls `POST /api/v1/admin/bootstrap` with a generated admin email,
     password and `INSTANCE_SETUP_TOKEN`, and gets the instance admin
     token;
  2. with it, creates project `sesh` and machine identity
     `sesh-gateway`, with universal auth and a client secret;
  3. gives that identity read and write on the project's `prod`
     environment, and nothing else.

  The admin token and the gateway's client id and secret are saved in
  `secrets.env`. The client id and secret are passed only to the gateway
  container.
- **The gateway:** logs in with universal auth, caches the access token
  until shortly before expiry, and reads or writes secrets at the root
  path of `prod`, keyed `<CONNECTOR>__<NAME>`. Infisical refuses secrets
  under folders that don't exist yet, so this layout needs no folders.
- **Network:** Infisical is on the stack's default network, because it
  needs Postgres and Redis. Its port is published on loopback only
  (`127.0.0.1:8222`), like the router's, so the Rust CLI can bootstrap it
  over HTTP without running code inside the container.
  - Other stack services can reach it, but they hold no identity that
    can read a credential.
  - Agents and executors aren't on that network.
- **Endpoints:** exact paths (v1, v3 or v4 secrets API) are pinned to the
  chosen image and checked by the stack test. The bootstrap API is
  documented: `docs/self-hosting/guides/automated-bootstrapping.mdx`.
- **Rationale:**
  - It's the adopted component (ADR 0001).
  - The bootstrap API allows set-up with no UI (constitution VIII).
  - Only the gateway's identity can read secrets (III).
- **Alternatives:** OpenBao or Vault would be new components; files on
  disk aren't a secret store.

## R6. Credential forms

- **Decision:** `sesh credential request <connector> <NAME>` calls the
  gateway's `/admin/forms` with the admin token. The gateway:
  - checks that the connector declares that credential;
  - creates a form row with a hashed secret and a 30-minute expiry;
  - returns `<approvals base URL>/form/<id>#<secret>`.

  The CLI prints the link, and never sees a value.
- **The form:** the page POSTs the value with the secret. The gateway
  writes it to Infisical, marks the form used, and audits
  `credential.fill`.
- **Storage:** form rows live in the `gateway` database, table
  `credential_forms`, with the same grants as `approvals`.
- **Missing credentials:** `sesh doctor`'s `credentials` check calls
  `/admin/credentials` (names and states only).

## R7. Test connectors

- `testgate.act({ note? })` keeps `requires_approval`. With approvals it
  now runs, returning `{done: true}`.
- New `testcred.check({})` declares credential `TOKEN`. It sends it as
  `Authorization: Bearer` to the fake upstream at `/testcred`, and returns
  only `{ok: true}`.
- The stub records the SHA-256 of the last bearer value it got, and
  serves it at `GET /testcred/last` (stub-only; the eval and stack tests
  compare it with the value they typed).
- Both connectors exist only with `SESH_GATEWAY_TEST_CONNECTORS=1`.

## R8. Eval: approvals answered by the harness

- **Decision:** the eval project gains its own `ntfy`, and the eval
  gateway gets approvals pointed at it. `sesh eval` reads the eval
  topic.
- **The `approval` task (deny):** the harness denies the request.
- **New `approve-act` task:**
  1. The harness approves the request.
  2. When the model answers without having run the call, the harness
     adds one user turn: "I've approved it."
  3. The task passes if `testgate.act` was allowed after the approval,
     and the answer says it was done.
- The household ntfy is never touched (FR-015).
- **Audit-entry shapes:** a request is audited `refused` with reason
  `approval pending`, and the approved run `allowed`. So the existing
  deny task's rule ("refused, reason approval…") still holds.
- **Rationale:** both outcomes, through the real path, and
  deterministic with the stub's scripts.

## R9. Rate limits and channel failures

- **Limits:** each agent can have at most 10 pending requests and 30 an
  hour, counted from the `approvals` table. Over the limit, the call is
  refused with "too many approval requests", and nothing is notified.
- **ntfy unreachable:** the request is marked denied with reason "approval
  channel unavailable", and the call is refused.
- **Infisical unreachable:** credentialed calls are refused with
  "credential store unavailable", and other calls work.
