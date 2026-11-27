# Contract: credentials

## Admin API (`:7002`, `Authorization: Bearer <SESH_GATEWAY_ADMIN_TOKEN>`)

- `POST /admin/forms {"connector": "testcred", "name": "TOKEN"}` →
  `{"url": "<base>/form/<id>#<secret>", "expires_at": "…"}`. Answers 404 if
  the connector doesn't declare that credential.
- `GET /admin/credentials` →
  `[{"connector": "testcred", "name": "TOKEN", "state": "missing"|"set"}]`.
  Never returns values.

## Form (`:7002`)

- `GET /form/<id>`: a page with one password field. It reads the secret
  from the URL fragment.
- `POST /form/<id> {"secret": "…", "value": "…"}`:
  - `200`: stored in Infisical; the form is marked used;
  - `403`: wrong secret;
  - `409`: already used;
  - `410`: expired.

  The value is written to Infisical and nowhere else, and the reply never
  echoes it.

## CLI

```
sesh credential request <connector> <NAME>   # prints the one-time link; never a value
sesh credential list                         # names and set/missing
```

`sesh doctor` gains a `credentials` check, which warns for each missing
credential of a granted connector.
