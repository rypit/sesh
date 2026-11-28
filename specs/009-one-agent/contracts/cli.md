# CLI contract: One agent end to end

## `sesh agent add <name> --connectors … --owner <owner> --runtime hermes`

New flag `--runtime hermes`. Without it, the agent is an identity only,
as today. With it, also creates the agent's API key in `secrets.env`.

Output (one more line):

```
added agent household-agent (owner household; connectors search, geo, mail, calendar, contacts, docs, feeds)
it will run as a hermes agent after the next `sesh up`
```

Errors: an unknown runtime → `unknown runtime "x"; the only one is hermes`.

## `sesh agent list`

A fourth column: the runtime (`hermes` or `-`).

## `sesh up`

For each agent with a runtime, it:
1. creates or updates its router key (aliases only);
2. writes `agents/<name>/config.yaml` and `agents/<name>.env` (0600),
   only when they changed;
3. adds `hermes-<name>`, its volume and its three networks to the
   generated overlay; adds the gateway's alias and `agent_listen`;
   adds the connection to Open WebUI;
4. starts it after the gateway and LiteLLM, and Open WebUI after it.

New lines:

```
created household-agent's router key (limited to the aliases)
agent household-agent is up (hermes, tools: the gateway's only)
```

A removed agent's router key is deleted from LiteLLM:

```
removed household-agent's router key
```

## `sesh doctor`

New check `agent-runtimes`, per agent with a runtime:
- the container is running and healthy;
- `GET /v1/toolsets` (with its API key, on loopback inside the
  runtime's container through `docker exec`) lists no enabled toolset
  (it lists only built-ins; research.md R12);
- its `config.yaml` matches what sesh would write.

Fail line: `agent-runtimes: household-agent has built-in toolsets enabled: terminal`.

`network-layout` includes the agent networks.
