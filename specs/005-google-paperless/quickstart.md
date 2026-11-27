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
