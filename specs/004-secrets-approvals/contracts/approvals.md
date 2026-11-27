# Contract: approvals

## To the agent (MCP `execute` result)

```json
{"status": "approval_pending",
 "approval": {"id": "…", "expires_at": "…"},
 "error": "testgate.act needs the person's approval. They have been asked. Once they approve, run the same call again with the same arguments."}
```

A re-run after a decision returns either the call's normal result
(approved), or `error` = "testgate.act was denied" or "the approval
expired".

## To the person (ntfy)

`POST http://ntfy/<topic>` as `sesh-gateway`, with this JSON:

```json
{"topic": "<topic>",
 "title": "Approve: <agent> — <method description>",
 "message": "<shown_text>",
 "priority": 4,
 "actions": [{"action": "view", "label": "Review", "url": "<base>/approve/<id>#<secret>", "clear": true}]}
```

## Approval page (`:7002`)

- **`GET /approve/<id>`:** an HTML page showing the request's
  `shown_text` and expiry, with Approve and Deny buttons. The page reads
  the secret from the URL fragment. The page itself holds no secret.
- **`POST /approve/<id>`,** body `{"secret": "…", "decision":
  "approve"|"deny"}`:
  - `200 {"state": …}` when decided;
  - `403` for a wrong secret;
  - `409` when already decided;
  - `410` when expired.

  Each answer is audited.
- **Refused here:** any `Authorization: Bearer` header carrying an agent
  token. The answer is 403, and the attempt is audited.
