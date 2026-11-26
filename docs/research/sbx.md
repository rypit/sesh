# Docker Sandboxes (sbx): reference notes

sesh runs browser sessions in sbx microVMs (ADR 0001). These notes record
sbx behavior that matters for that phase. Sources are the docs' source in
[docker/docs](https://github.com/docker/docs) (`content/manuals/ai/sandboxes/`
and `content/manuals/ai/governance/`) and issues in
[docker/sbx-releases](https://github.com/docker/sbx-releases), read
2026-11-20 to 2026-11-26 against sbx 0.54.0 (released 2026-11-16).
Items marked **verify** need confirming on the reference Mac.

## Isolation

- Each sandbox is a microVM with its own kernel (Hypervisor.framework on
  macOS) and its own Docker engine. "The hypervisor boundary is the
  isolation control, not in-VM privilege separation"; the agent runs as
  non-root with sudo inside the VM.
- Platforms: macOS 14+ on Apple Silicon (`brew install docker/tap/sbx`),
  Windows 11 x64, Ubuntu 24.04+ with KVM.
- sbx requires a Docker sign-in. **Verify** its login and telemetry terms.

## Network

- Every outbound TCP connection, HTTP and SSH included, goes through a
  proxy on the host and is checked against policy. UDP needs an
  experimental feature; ICMP is always blocked.
- A default preset must be chosen first: `sbx policy set-default
  <allow-all|balanced|deny-all>`. It is a global setting for the sbx
  install; `balanced` is deny-all plus common developer services.
- Rules: `sbx policy allow network <host>[:port] [--method GET,POST]
  [--path '/x/**'] [--query k=v] [--sandbox <name>]`. Deny wins. HTTP
  rules match hostnames only.
- Host services: allow `localhost:<port>`, reach
  `host.docker.internal:<port>` from inside.
- **Kits add their own allow rules**, which a deny-all default does not
  remove; block them with explicit per-sandbox deny rules. Inspection:
  `sbx policy ls` (and reportedly `--source kit`; **verify**).
- `--deny-network` rules given at create are applied after start, so the
  first requests can bypass them (issue #889): apply policy before
  starting work.
- `sbx policy check` and `sbx policy log` don't evaluate method/path
  rules; test those directly. The log's CLI fields are in
  `governance/local/inspect.md` (**verify**).
- Inbound: `sbx port publish <sandbox> <host>:<guest>` (needed for a
  noVNC live view).

## Credentials

- Built-in secret services (`sbx secret set <service>`): anthropic,
  cursor, droid, github, google, groq, mistral, nebius, openai,
  openrouter, xai; stored in the OS keychain.
- `sbx secret set-custom --host H --env VAR (--value-stdin | --command …
  | --ref …) [--sandbox S]` (experimental): the sandbox sees a
  placeholder, substituted in HTTP headers only, and in **any** header to
  a bound host (issue #707).
- "Environment variables are readable by processes inside the sandbox."
- **SSH agent forwarding is on by default**: any process inside can ask
  the forwarded agent to authenticate or sign. Turn it off for browser
  VMs.
- sbx "environment files" can run lifecycle and credential commands on
  the host with the user's privileges; sesh must not use them for
  untrusted input.

## Filesystem and lifecycle

- `sbx run` mounts the current directory read-write ("There is no
  boundary between the agent's edits and your host filesystem"); extra
  mounts accept `:ro`. Mountless sandboxes: `sbx create --name <n> <kit>`.
  Browser VMs should be mountless.
- `sbx cp <src> <sandbox>:<abs-path>`, `sbx exec -it <sandbox> <cmd>`.
- Sandboxes persist (installed packages, images, files) across stop and
  start; `sbx rm` deletes; `sbx run --rm` removes on exit.
- Clipboard: processes can write to the host clipboard (not read it).

## Open checks for the browser phase

- sign-in and telemetry terms; kit rule inspection; policy-log fields;
  SSH-forwarding off; live-view port publishing; memory per VM (budget
  assumes 1–2 GB each).
