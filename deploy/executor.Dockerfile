# The untrusted per-agent executor: the adopted Code Mode runtime in workerd,
# run by a small supervisor that restarts it when a script overruns its
# deadline (ADR 0004). It holds no secrets and its only route is the gateway.
# Build from the repository root:
#   docker build -f deploy/executor.Dockerfile -t sesh-executor:dev .
FROM rust:1.97-slim-bookworm AS supervisor
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p sesh-executor-supervisor

FROM node:24-trixie-slim AS bundle
WORKDIR /src
COPY executor/package.json executor/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY executor/src ./src
RUN npm run build \
 && cp "$(node -p 'require.resolve("@cloudflare/workerd-linux-" + (process.arch === "x64" ? "64" : "arm64") + "/bin/workerd")')" /workerd

FROM debian:trixie-slim
COPY --from=bundle /workerd /usr/local/bin/workerd
COPY --from=supervisor /src/target/release/sesh-executor-supervisor /usr/local/bin/sesh-executor-supervisor
COPY --from=bundle /src/dist /opt/sesh-executor/dist
COPY executor/workerd.capnp.template /opt/sesh-executor/workerd.capnp.template
USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/sesh-executor-supervisor"]
