# Prior art

## n8n Nod

[n8n-io/nod](https://github.com/n8n-io/nod) (Apache-2.0, research
preview; README read 2026-11-20) is an open-source personal agent that
does errands on real websites and apps, through a desktop app, Telegram
or WhatsApp. The architecture names its browser as a candidate to adopt.

- **Shape**: a Node server plus Docker containers (Linux workspace,
  Chromium, PostgreSQL), using the pi agent runtime. Containers, not
  microVMs; the README describes no egress allowlist.
- **Keys out of the sandbox**: "Provider keys never enter the workspace
  or the browser"; a vault fills passwords and cards on websites.
- **Policy in the server**: "Policy lives in the server, not the prompt."
- **Untrusted content as data**: app, web and browser results reach the
  model "marked as data, not instructions"; memory learns only from the
  user's own messages, never from pages it reads.
- **Approvals by model judgement**: actions outside the request wait for
  a yes, but the check is a model's judgement; its own test reports one
  model letting 8 unsafe actions through out of 240 decisions.
- **Telemetry on by default**, and app connections go through an
  n8n-hosted service by default.

**For sesh**: "results as data" and user-only memory match the
architecture. Approvals in sesh stay deterministic (the gateway decides
what needs a yes); a model's judgement could only add checks on top.
