// sesh executor host Worker (specs/002-gateway-core/contracts/executor.md).
//
// Untrusted by design: it holds no secrets, and the gateway re-checks every
// call it forwards. Scripts run in dynamic workers with no network
// (Code Mode's default); the only outbound route is the GATEWAY binding.

import { DurableObject } from "cloudflare:workers";
import { CodemodeConnector, createCodemodeRuntime, DynamicWorkerExecutor } from "@cloudflare/codemode";
export { CodemodeRuntime } from "@cloudflare/codemode";

function forwarder(env, capability, descriptor) {
  // One connector class per descriptor; each method forwards to the gateway.
  class Forwarder extends CodemodeConnector {
    name() {
      return descriptor.name;
    }
    instructions() {
      return descriptor.instructions;
    }
    tools() {
      const tools = {};
      for (const [method, m] of Object.entries(descriptor.methods)) {
        tools[method] = {
          description: m.description,
          inputSchema: m.input,
          requiresApproval: m.requires_approval === true,
          execute: async (args) => {
            const resp = await env.GATEWAY.fetch("http://gateway/call", {
              method: "POST",
              headers: { "content-type": "application/json", "x-sesh-capability": capability },
              body: JSON.stringify({ connector: descriptor.name, method, args: args ?? {} }),
            });
            const body = await resp.json().catch(() => ({ ok: false, error: "gateway sent an unreadable reply" }));
            if (!body.ok) throw new Error(body.error ?? `call refused (${resp.status})`);
            return body.result;
          },
        };
      }
      return tools;
    }
  }
  return Forwarder;
}

export class Runtime extends DurableObject {
  async execute(req) {
    const connectors = (req.connectors ?? []).map((d) => {
      const C = forwarder(this.env, req.capability, d);
      return new C(this.ctx, this.env);
    });
    const runtime = createCodemodeRuntime({
      ctx: this.ctx,
      executor: new DynamicWorkerExecutor({ loader: this.env.LOADER, timeout: req.deadline_ms ?? 30000 }),
      connectors,
    });
    const out = await runtime.execute({ code: String(req.code ?? "") });
    if (out.status === "completed") return { status: "completed", result: out.result ?? null, logs: out.logs ?? [] };
    if (out.status === "paused") return { status: "paused", pending: out.pending ?? [] };
    return { status: "error", error: String(out.error ?? "script failed"), logs: out.logs ?? [] };
  }

  async fetch(request) {
    try {
      return Response.json(await this.execute(await request.json()));
    } catch (e) {
      return Response.json({ status: "error", error: String(e?.message ?? e) });
    }
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/health") return new Response("ok");
    if (url.pathname !== "/execute" || request.method !== "POST") return new Response("not found", { status: 404 });
    const stub = env.RUNTIME.get(env.RUNTIME.idFromName("agent"));
    return stub.fetch(request);
  },
};
