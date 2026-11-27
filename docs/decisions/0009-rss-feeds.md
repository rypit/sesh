# 0009. The gateway fetches feeds from public hosts, behind an address filter

- Status: proposed
- Date: 2026-11-27
- Feature: specs/006-rss-feeds

## Context

Feeds (RSS and Atom) are published on arbitrary hosts. Until now the
gateway reached a fixed set of upstreams (SearXNG, Kiwix, Open-Meteo,
Nominatim, Google, Paperless). A feed address is chosen by a person,
or asked for by an agent, so the gateway would make requests to
addresses no one fixed in advance. That opens two threats:

- **Server-side request forgery:** the gateway sits on the stack's
  `default` network with Postgres, Redis, Infisical, ntfy and LiteLLM,
  and on the internal `search` and `docs` networks. A feed address (or
  a redirect) pointing at one of them, at the host, at the tailnet or at
  the home network would let an agent probe or read inside.
- **Exfiltration:** an agent that can make the gateway fetch any URL can
  put data in the URL and send it to anyone's server.

## Decision

1. **Feeds are configuration, held by the gateway.** Each feed belongs
   to an owner (a user, or the Household) and lives in the gateway's
   `feeds` table, which only the gateway writes. The operator changes it
   with `sesh feed …` through the gateway's admin endpoints; each change
   is audited. Agents read their owner's feeds and the Household's
   (`owners::Scope.readable`, ADR 0008).
2. **An agent can only ask to add a feed.** `feeds.add` is
   approval-gated (ADR 0007): the person sees the exact address and
   name, and only then does the gateway store it. Invalid, internal,
   duplicate or over-limit requests are refused before anyone is asked.
   Feed content is never configuration.
3. **One module fetches** (`connectors/feeds.rs`), and only:
   - `http` and `https` URLs, with a host and no credentials;
   - after resolving the host itself and refusing if *any* address is
     loopback, private, link-local, CGNAT, multicast, unspecified,
     broadcast, reserved or documentation space, IPv4 or IPv6
     (including IPv4-mapped, NAT64 and 6to4 forms);
   - by connecting to the address it checked (pinned per request), so
     DNS rebinding can't swap it;
   - following at most 3 redirects by hand, each checked the same way;
   - with no proxy, a 2 MB cap, a 10 s limit, and a 15-minute cache per
     URL (failures cached for a minute), so agents can't hammer a
     publisher or use the cache to time internal services.
4. **Errors name the problem, never an address** the gateway resolved.
5. **A test-only allowance:** `feeds_allow_hosts` names Compose
   services (single-label names only, never a dotted name or an IP)
   whose private address the filter accepts, for that hop only. The
   gateway honours it only when `SESH_GATEWAY_TEST_CONNECTORS=1`, which
   only the stub overlay and the eval project set; `sesh up` fills it
   only in stub mode (`stub-llm`), the eval with `fakes`, and doctor
   fails if it is set outside stub mode. A redirect from an allowed
   host is checked as usual.

## Why the allowance is safe

- A household stack never runs with test connectors on: they come only
  from `deploy/compose.stub.yaml` (stub mode) and the eval project,
  both of which have no real accounts or household data.
- Even there it names services, not addresses, and covers one hop: a
  redirect or second name is checked normally, which the stack test
  exercises (a redirect from the fake to loopback is refused).

## Consequences

- The gateway gains its first outbound path to hosts no one fixed in
  advance. The address filter is now a control-plane guarantee with its
  own negative tests (every range, v4 and v6, and redirects).
- A personal agent's approved feed is visible only to that owner's
  agents; a Household feed to everyone's.
- Constitution VII names its outbound exceptions; fetching feeds the
  household chose is recorded as a justified exception in the plan's
  Complexity Tracking. Whether to name it in VII is the operator's call
  (an amendment).
- The browser's `read(url)` (feature 010) will need the same filter in
  its sandbox; this module's range list is the reference.
