# Quickstart and validation: Gateway core

Builds on feature 001's quickstart. Contracts: [mcp](./contracts/mcp.md),
[executor](./contracts/executor.md), [connectors](./contracts/connectors.md),
[cli](./contracts/cli.md).

```bash
docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
docker build -f deploy/gateway.Dockerfile -t sesh-gateway:dev .
docker build -f deploy/executor.Dockerfile -t sesh-executor:dev .
sesh init --stub --home 59.91,10.75
sesh agent add test --connectors search,geo
sesh up
sesh doctor
make stack-test
```

Expect: an MCP client using the token in
`~/.local/share/sesh/agents/test.token` against `http://127.0.0.1:7000/mcp`
can `search`, `describe` and `execute` a script that calls `search.web`
and `geo.forecast`; every call appears in the audit log
(`docker compose -p sesh exec postgres psql -U sesh -d gateway -c 'select * from audit'`).
The stack tests run every negative case in spec.md User Story 2–4.
