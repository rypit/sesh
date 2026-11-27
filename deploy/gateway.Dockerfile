# The sesh gateway: the trusted MCP server agents talk to (ADR 0004).
# Build from the repository root:
#   docker build -f deploy/gateway.Dockerfile -t sesh-gateway:dev .
FROM rust:1.97-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Keep only the binary: the target directory and the crate registry
# would otherwise stay in this layer's build cache (several GB).
RUN cargo build --release -p sesh-gateway \
 && cp target/release/sesh-gateway /sesh-gateway \
 && rm -rf target /usr/local/cargo/registry

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /sesh-gateway /usr/local/bin/sesh-gateway
USER 65534:65534
EXPOSE 7000 7001
ENTRYPOINT ["/usr/local/bin/sesh-gateway"]
