# Quickstart: One agent end to end

```sh
sesh agent add household-agent --owner household \
  --connectors search,geo,mail,calendar,contacts,docs,feeds --runtime hermes
sesh up
sesh doctor            # agent-runtimes: ok
```

Open WebUI (http://127.0.0.1:3000) now lists `household-agent`. Ask:

- "What's the weather at home tomorrow?" (the gateway's `geo.forecast`)
- "Search the web for the opening hours of the library."
- "Anything new from the electricity company?" (needs `sesh google connect household`)
- "What's new on my feeds?"
- "Send Kari a reminder about Saturday." → an approval on the Household topic.

Remove:

```sh
sesh agent remove household-agent
sesh up                # container, networks and Open WebUI entry gone
```

In development, `STUB_SCRIPT=agent sesh up` makes the stub model play
the scripted conversation; `make stack-test STACK_TESTS=agent_stack`
runs the test.
