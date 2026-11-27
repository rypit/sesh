# The untrusted per-agent executor: the adopted Code Mode runtime in workerd,
# run by a small supervisor that restarts it when a script overruns its
# deadline (ADR 0004). It holds no secrets and its only route is the gateway.
# Build from the repository root:
#   docker build -f deploy/executor.Dockerfile -t sesh-executor:dev .
FROM rust:1.97-slim-bookworm AS supervisor
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Dependencies first, in their own layer: it stays cached until a manifest
# or Cargo.lock changes, so a code change rebuilds only sesh's own crates.
# Every workspace member's manifest is needed; placeholder sources stand in.
COPY crates/sesh-cli/Cargo.toml crates/sesh-cli/
COPY crates/sesh-core/Cargo.toml crates/sesh-core/
COPY crates/sesh-executor-supervisor/Cargo.toml crates/sesh-executor-supervisor/
COPY crates/sesh-gateway/Cargo.toml crates/sesh-gateway/
COPY crates/sesh-stub-llm/Cargo.toml crates/sesh-stub-llm/
RUN for d in crates/*/; do mkdir -p "$d/src" && echo 'fn main() {}' > "$d/src/main.rs" && : > "$d/src/lib.rs"; done \
 && cargo build --release -p sesh-executor-supervisor \
 && rm -rf crates/*/src
COPY crates ./crates
# COPY keeps the context's file times, which can be older than the
# placeholders: touch them so cargo rebuilds sesh's crates. Keep only the
# binary in this layer.
RUN find crates -name '*.rs' -exec touch {} + \
 && cargo build --release -p sesh-executor-supervisor \
 && cp target/release/sesh-executor-supervisor /sesh-executor-supervisor \
 && rm -rf target

FROM node:24-trixie-slim AS bundle
WORKDIR /src
COPY executor/package.json executor/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY executor/src ./src
RUN npm run build \
 && cp "$(node -p 'require.resolve("@cloudflare/workerd-linux-" + (process.arch === "x64" ? "64" : "arm64") + "/bin/workerd")')" /workerd

FROM debian:trixie-slim
# workerd refuses to start without a system CA store, even though the
# executor has no route to the internet.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=bundle /workerd /usr/local/bin/workerd
COPY --from=supervisor /sesh-executor-supervisor /usr/local/bin/sesh-executor-supervisor
COPY --from=bundle /src/dist /opt/sesh-executor/dist
COPY executor/workerd.capnp.template /opt/sesh-executor/workerd.capnp.template
USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/sesh-executor-supervisor"]
