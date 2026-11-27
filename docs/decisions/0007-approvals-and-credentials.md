# 0007. Approvals by fingerprint and retry; credentials by one-time form

- Status: accepted
- Date: 2026-11-27
- Feature: specs/004-secrets-approvals

## Context

Constitution III requires that only the gateway uses credentials, and
that credentials enter through a one-time form. Constitution V requires a
human yes for side effects, on a channel no agent can use, with text the
gateway writes. ADR 0004 made the executor untrusted, and planned
approvals as "abort and replay".

## Decision

1. **Approvals are decided at `/call`.** The executor no longer pauses.
   A gated call runs only if an approved, unused, unexpired approval
   matches the SHA-256 fingerprint of (agent, connector, method,
   canonical arguments). Otherwise the gateway creates a request, notifies
   the person, and returns `approval_pending`. The agent re-runs the call
   after the decision. An approval runs exactly once.
2. **The person decides on the approval page,** reached from an ntfy
   "view" action that holds a per-request secret in the URL fragment. The
   page and its POST are on a gateway listener (`:7002`) that is
   loopback- and Tailscale-only, refuses agent tokens, and is never on an
   agent or executor network.
3. **Approval state is in Postgres.** The gateway role may insert,
   select and update the approvals and forms tables. The audit table stays
   insert-only.
4. **Credentials live in Infisical.** Only the gateway's machine identity
   can read or write them. The CLI bootstraps Infisical (instance admin
   token, project, gateway identity) and creates one-time form links
   through an admin endpoint, but never handles a credential value.
5. **Connectors declare their credentials.** The gateway sends each one
   only to its own connector's upstream.

## Consequences

- An agent can never cause a side effect without a fresh human decision
  on the exact call. A looping agent hits the rate limit, not the
  person's patience.
- The gateway now talks to Infisical and ntfy. Both are already in the
  architecture diagram, so the diagram doesn't change.
- The CLI holds the Infisical admin token in `secrets.env`. It is the
  operator's credential and is never passed to a container.
- Code Mode's durable resume goes unused. A script after approval is
  simply run again.
