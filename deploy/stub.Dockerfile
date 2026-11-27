# Builds the stub model server used by development and CI (never by a
# household deployment). Build from the repository root:
#   docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
FROM rust:1.97-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Keep only the binary: the target directory and the crate registry
# would otherwise stay in this layer's build cache (several GB).
RUN cargo build --release -p sesh-stub-llm \
 && cp target/release/sesh-stub-llm /sesh-stub-llm \
 && rm -rf target /usr/local/cargo/registry

FROM debian:bookworm-slim
COPY --from=build /sesh-stub-llm /usr/local/bin/sesh-stub-llm
USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/sesh-stub-llm"]
