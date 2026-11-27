# Quickstart and validation: Foundation stack

Prerequisites: Docker (Desktop on the Mac), Rust (to build `sesh` until
packaged), Tailscale on the server. Details: [contracts/cli.md](./contracts/cli.md),
[contracts/config.md](./contracts/config.md), [contracts/stack.md](./contracts/stack.md).

## Without a real model (any machine with Docker)

```bash
cargo install --path crates/sesh-cli
docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
sesh init --stub --admin-email you@example.com
sesh up
sesh doctor            # all pass; tailscale-serve may warn
make stack-test        # negative tests (FR-012)
```

Expect: Open WebUI at http://127.0.0.1:3000; the admin password is in
`~/.local/share/sesh/secrets.env`; `agent-main` replies "stub reply".

## With your inference server

```bash
sesh init --inference-url http://<server>:<port>/v1 --model <model-name>
sesh up
sesh doctor
tailscale serve --bg 3000
```

`tailscale serve` exposes Open WebUI over HTTPS on your tailnet only; the
stack itself stays on loopback. This step is manual and is not yet
verified by CI (it needs a real tailnet).

Expect (US1–US4):
- From your phone on the tailnet, open the Tailscale HTTPS address, sign
  in, chat with `agent-main`.
- From another device on your Wi-Fi, `http://<server-lan-ip>:3000` is
  refused.
- A non-admin user (created by the admin) sees only `agent-main`.
- `sesh doctor` passes; stop the inference server and it fails
  `alias-answers`.

## Switch inference servers

```bash
sesh init --inference-url http://<other>/v1 --model <name>
sesh up                # restarts only the router
```
