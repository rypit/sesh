# Implementation Plan: Network isolation

**Branch**: `claude/network-isolation` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/008-network-isolation/spec.md`

## Summary

- **No `default` network.** Every service in `deploy/compose.yaml`
  names its networks explicitly, and none names `default`.
- **Database networks:** `litellm-db`, `openwebui-db`, `gateway-db`,
  `infisical-db` and `paperless-db`, all internal, plus feature 006's
  `feeds-db`. Postgres joins only these six.
- **Pair networks**, all internal:
  - `models`: Open WebUI and LiteLLM;
  - `secrets`: the gateway and Infisical;
  - `notify`: the gateway and ntfy.
- **Edge networks**, non-internal, one member each: `webui-edge`,
  `litellm-edge`, `gateway-edge`, `infisical-edge` and `ntfy-edge`.
  Each carries its service's published loopback port, and its outbound
  access where needed: LiteLLM to the inference server, the gateway to
  Google, Open-Meteo and Nominatim, ntfy to push relays.
- **Stub mode:** `compose.stub.yaml` adds an internal `stub` network
  joining `stub-llm`, LiteLLM and the gateway. `stub-llm` stays on
  `feeds-egress` for Miniflux. It leaves `default`.
- **One source of truth:** a layout table in `sesh-cli`
  (`crates/sesh-cli/src/layout.rs`) holds every service's networks and
  the allowed reachability pairs. It drives three checks:
  - a unit test that `compose.yaml` matches it;
  - the doctor's `network-layout` check;
  - the `network_stack` reachability test.
- **The gateway listener** is unchanged in code. It binds loopback plus
  the default-route address, and with one non-internal network
  (`gateway-edge`) that address is the edge network's. A unit test pins
  this rule, and ADR 0011 records why it holds.
- **Upgrade:** `sesh up` already recreates containers whose networks
  changed (`--remove-orphans`). It then removes `sesh_default` when no
  container uses it.
- **ADR 0011**, architecture text, and the roadmap renumbered (009
  Google Docs, 010 memory, 011 one agent, 012 browser, 013 household).

## Technical Context

**Language/Version**: Rust 1.97 (ADR 0002), with
`#![forbid(unsafe_code)]`; Compose YAML.

**Primary Dependencies**: no new ones. Docker Compose v2 and the
existing images.

**Storage**: none changes. Volumes are untouched, and containers are
recreated.

**Testing**:
- `cargo test`: the layout table against `compose.yaml` and
  `compose.stub.yaml`, the doctor's check, and the listener rule.
- `make stack-test`: a new `network_stack`, plus the existing seven.

**Target Platform**: Linux with Docker; the Mac mini later (ADR 0001).

**Project Type**: a Compose stack and its admin CLI.

**Performance Goals**: none. Bridge networks add no measurable
overhead; there are about 14 networks, against 10 today.

**Constraints**:
- CI runs with containers' internet blocked.
- Published ports stay on loopback.
- The eval project is unchanged.

**Scale/Scope**: 12 services and about 14 networks in the household
stack. Executors add one network each, as today.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check |
|---|---|
| I. Untrusted adopted services | **Strengthened.** A compromised service reaches only its row of the reachability table. |
| II. One way out | **Strengthened.** Agents still reach resources only through the gateway. Executor networks are unchanged. |
| III–V. Secrets, per-user isolation, approvals | Unchanged. The gateway still reaches Infisical and ntfy, on their pair networks. |
| VI. Agent content is untrusted | Unchanged. |
| VII. Local-first | Unchanged. Outbound access is only through each service's edge network, as today. Nothing new leaves the server. |
| VIII. Adopt before build | No new component; it's configuration plus tests. |
| IX. Swappable inference | LiteLLM still reaches any inference server, through its edge network. |
| X. Every guarantee has an automated check | The reachability test, the layout unit test and the doctor check. |

No violations. Complexity Tracking is empty.

Post-design re-check (after Phase 1): still no violations. The listener
rule keeps its current code, the contracts add no new trusted-plane
interfaces, and the doctor check reads Docker metadata only.

## Project Structure

### Documentation (this feature)

```text
specs/008-network-isolation/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── networks.md      # the layout and reachability table
│   └── cli.md           # doctor network-layout, sesh up behaviour
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
deploy/
├── compose.yaml          # every service's networks; no `default`
└── compose.stub.yaml     # the `stub` network
crates/sesh-cli/src/
├── layout.rs             # NEW: the layout and reachability table
├── up.rs                 # removes sesh_default after up; overlay unchanged
├── doctor.rs             # network-layout check
└── host/compose.rs       # Compose::remove_network_if_unused
crates/sesh-cli/tests/stack.rs   # network_stack; probe helpers
crates/sesh-gateway/src/listen.rs # unit test pinning the listener rule
docs/decisions/0011-network-isolation.md
docs/architecture/solutions-architecture.md
.github/workflows/ci.yml  # network_stack joins a stack group
```

**Structure Decision**: no new crate. The layout table lives in
`sesh-cli`, which owns Compose; the gateway only gets a test.

## Complexity Tracking

None.
