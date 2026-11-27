# 0009. Feeds through Miniflux, an adopted feed service behind the gateway

- Status: proposed
- Date: 2026-11-27
- Feature: specs/006-rss-feeds

## Context

Feeds (RSS and Atom) are published on arbitrary hosts, at addresses a
person chooses or an agent asks for. Whatever fetches them makes
requests no one fixed in advance, which opens two threats:

- **Server-side request forgery:** a fetcher on the stack's `default`
  network (Postgres, Redis, Infisical, ntfy, LiteLLM, Open WebUI) could
  be pointed at them.
- **Exfiltration:** an agent that can have any URL fetched can put data
  in it and send it anywhere.

A first draft had the gateway fetch feeds itself, behind its own
address filter, DNS pinning, redirect checks, caps, cache and parser.
That made the trusted plane the fetcher and grew a parser and an SSRF
guard in it (constitution I, VIII), and made RSS a special case among
resources.

## Decision

1. **RSS follows the resource flow of search and documents:** an
   adopted service, Miniflux, behind the gateway, reached through
   exactly one module (`connectors/feeds.rs`), used by agents through
   code mode. The gateway never fetches a feed address.
2. **Miniflux's networks:** `feeds` (internal, gateway and Miniflux
   only), `feeds-egress` (Miniflux alone, to publishers), `feeds-db`
   (internal, Miniflux and Postgres only). Miniflux is not on `default`.
   Its database is `miniflux` in the shared Postgres, reached with its
   own `miniflux` login, which owns only that database and has no
   superuser, database or role rights.
3. **One Miniflux user per owner** (`sesh-<owner>`), created by the
   gateway as Miniflux's admin and matched by exact name. The gateway
   signs in as that user with a per-process random password it keeps
   in memory, and filters every feed and entry again by the owner's
   Miniflux id: a personal agent reads its owner's feeds and the
   Household's; the Household agent reads only the Household's.
4. **An agent can only ask to add a feed.** `feeds.add` is
   approval-gated (ADR 0007), because Miniflux would fetch an
   agent-chosen address: the person sees the exact address and name.
   Invalid, duplicate or over-limit requests are refused before anyone
   is asked. The operator adds feeds with `sesh feed add`, through the
   gateway's admin endpoints, audited.
5. **The executor port accepts a call only from the calling agent's
   own executor.** The capability names the agent; the gateway resolves
   that agent's executor host and refuses (and audits) a call from any
   other address. An adopted service that shares a network with the
   gateway, or another agent's executor, can't use a capability even if
   it obtained one.
6. **Constitution VII is amended** (1.3.0): the named exception for
   search engines also covers feed publishers, reached only through the
   feed service.

## Consequences

- Fetching, parsing and storage are adopted, not built; the gateway
  keeps no feed table and no fetch code.
- **What Miniflux can still reach:** the internet; Postgres on
  `feeds-db`, as a login that can open only its own database; and the
  gateway's executor port on `feeds`, which refuses every call that
  doesn't come from the calling agent's executor (decision 5). A feed
  address can make Miniflux send a GET with no credentials, nothing
  more. SearXNG has the same shape on `search`.
- Miniflux polls feeds on its own (hourly by default), so items are as
  fresh as its last poll; publishers see the server's address, as with
  any reader.
- A Miniflux bug in per-user filtering would not leak items: the
  gateway checks owners itself, and unit tests use a fake Miniflux that
  returns every user's data.
- Miniflux's web UI isn't published; people manage feeds with the CLI.
