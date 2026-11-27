# The sesh gateway: the trusted MCP server agents talk to (ADR 0004).
# Build from the repository root:
#   docker build -f deploy/gateway.Dockerfile -t sesh-gateway:dev .
FROM rust:1.97-slim-bookworm AS build
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
 && cargo build --release -p sesh-gateway \
 && rm -rf crates/*/src
COPY crates ./crates
# COPY keeps the context's file times, which can be older than the
# placeholders: touch them so cargo rebuilds sesh's crates. Keep only the
# binary in this layer.
RUN find crates -name '*.rs' -exec touch {} + \
 && cargo build --release -p sesh-gateway \
 && cp target/release/sesh-gateway /sesh-gateway \
 && rm -rf target

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /sesh-gateway /usr/local/bin/sesh-gateway
USER 65534:65534
EXPOSE 7000 7001
ENTRYPOINT ["/usr/local/bin/sesh-gateway"]
