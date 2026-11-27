# Builds the stub model server used by development and CI (never by a
# household deployment). Build from the repository root:
#   docker build -f deploy/stub.Dockerfile -t sesh-stub-llm:dev .
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
 && cargo build --release -p sesh-stub-llm \
 && rm -rf crates/*/src
COPY crates ./crates
# COPY keeps the context's file times, which can be older than the
# placeholders: touch them so cargo rebuilds sesh's crates. Keep only the
# binary in this layer.
RUN find crates -name '*.rs' -exec touch {} + \
 && cargo build --release -p sesh-stub-llm \
 && cp target/release/sesh-stub-llm /sesh-stub-llm \
 && rm -rf target

FROM debian:bookworm-slim
COPY --from=build /sesh-stub-llm /usr/local/bin/sesh-stub-llm
USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/sesh-stub-llm"]
