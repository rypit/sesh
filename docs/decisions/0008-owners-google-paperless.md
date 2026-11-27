# 0008. Agents have owners; the gateway calls Google and Paperless itself

- Status: accepted
- Date: 2026-11-27
- Feature: specs/005-google-paperless

## Context

Mail, calendar and documents are per person (constitution IV), and
Google grants are credentials (III). The architecture's catalog listed a
Google MCP server, which would hold OAuth tokens outside the gateway.

## Decision

1. **Every agent has an owner:** a user, or the Household. The gateway
   derives every per-user scope from the token's agent → owner, at call
   time:
   - the Google grant;
   - the Paperless owner ids;
   - the approval topic.

   No method takes a user or account argument.
2. **The gateway calls the Google APIs itself,** through its Google
   module. The household's own OAuth client and a consent link
   (one-time, PKCE, single-use state) put the refresh token straight
   into Infisical under the owner. There is no Google MCP server.
3. **The gateway reaches Paperless,** through one module, as the
   Paperless admin. It filters every search and read by owner, and sets
   the owner on every upload. Attachment bytes move inside the gateway,
   by reference, and never enter model context.
4. **Approvals go to the owner's topic.** Household approvals go to the
   Household topic, which every user reads.

## Consequences

- Removing a Google MCP server removes a component that would have held
  credentials. The catalog row changes; the diagram's boxes don't.
- The gateway holds the Paperless admin credential. A gateway bug in
  owner filtering would expose documents, so negative tests cover every
  cross-user read.
