# 0006. Model evals run in the admin CLI against a separate eval gateway

- Status: accepted
- Date: 2026-11-27
- Feature: specs/003-model-evals

## Context

Before an agent depends on a model, the operator needs to know whether
that model can use sesh's resources through the gateway. The check has
to:
- go through the same alias and router agents use;
- give the model exactly the gateway's tools;
- be repeatable, needing no internet;
- leave the household's gateway, agents and audit log untouched.

## Decision

- **The harness:** `sesh eval`, in the admin CLI, runs a minimal agent
  loop. It reaches the router's chat API with an alias-only virtual key,
  and a gateway's MCP agent endpoint through one module
  (`host/mcp.rs`).
- **A separate project per run:** each run starts a short-lived Compose
  project, `sesh-eval`, removed when the run ends:
  - `gateway-eval`, with upstreams pointed at fakes, the test-only
    approval connector, and one agent with search, geo and testgate;
  - `executor-eval`, on its own internal network with only the gateway
    (ADR 0004);
  - `fakes`, on another internal network with only the gateway.
- **The audit sink:** the gateway gains a file audit sink, accepted only
  under `/var/log/sesh-eval/` and set only by the eval project. The CLI
  reads it to score which connector methods ran.
- **Untrusted output:** model output is untrusted. The harness stores and
  shows it labelled as such, and never acts on it.

## Consequences

- The CLI is now an MCP client of a gateway. It is still trusted-plane
  Rust, and holds no credential that the household gateway accepts.
- The household gateway still writes its audit only to Postgres
  (insert-only role). The file sink can't be pointed elsewhere.
- The eval tests the model and the gateway's tools, not an agent
  framework. Hermes (Phase 7) is evaluated separately.
- Each new connector adds tasks to the suite in the phase that adds it.
