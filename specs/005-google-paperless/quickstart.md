# Quickstart: Google and Paperless

```bash
sesh init --stub && sesh up
sesh user add ana && sesh user add ben
sesh agent add ana-agent --connectors mail,calendar,contacts,docs --owner ana
sesh credential request google CLIENT_ID     # and CLIENT_SECRET (stub mode: any values)
sesh google connect ana                       # open the link; in stub mode the fake consent returns at once
```

Then, as `ana-agent`:
- `mail.search({query: "invoice"})` returns Ana's fake mail.
- `mail.read` returns attachment refs.
- `docs.upload({ref, title: "Invoice"})` files the attachment into Ana's
  Paperless.
- `mail.send(...)` waits for approval on Ana's topic.

`ben-agent` sees none of Ana's mail or documents.

A real Google account needs a Google Cloud OAuth client ("Web
application"), with `<approvals_base_url>/google/callback` as an
authorized redirect URI.

**Keep the OAuth client "In production".** Gmail read access is a
restricted scope: while the client's publishing status is "Testing",
Google expires its refresh tokens after 7 days, and everyone would have
to reconnect weekly. An unverified client "In production" works for a
household's few users (they see an "unverified app" warning once, at
consent) and its grants don't expire. `sesh doctor` warns when a grant
was revoked.
