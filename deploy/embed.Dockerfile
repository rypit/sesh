# The embedding service for memory (specs/011, ADR 0015):
# text-embeddings-inference (CPU) with intfloat/multilingual-e5-small
# (MIT, 384 dimensions, about 100 languages) baked in, so it needs no
# network at runtime.
#
#   docker build -f deploy/embed.Dockerfile -t sesh-embed:dev .
#
# It holds no secrets and stores nothing; only the gateway reaches it.
ARG TARGETARCH

FROM debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e AS model
ARG MODEL=intfloat/multilingual-e5-small
# The files are pinned by SHA-256 in deploy/embed/model.sha256 (T018).
ARG MODEL_REVISION=main
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY deploy/embed/model.sha256 /model.sha256
RUN mkdir -p /model/1_Pooling && cd /model \
 && for f in config.json tokenizer.json tokenizer_config.json special_tokens_map.json \
      modules.json sentence_bert_config.json 1_Pooling/config.json model.safetensors; do \
      curl -fsSL --retry 5 -o "$f" "https://huggingface.co/${MODEL}/resolve/${MODEL_REVISION}/$f"; \
    done \
 && sha256sum config.json tokenizer.json tokenizer_config.json special_tokens_map.json \
      modules.json sentence_bert_config.json 1_Pooling/config.json model.safetensors \
 && if grep -qv '^#' /model.sha256; then grep -v '^#' /model.sha256 | sha256sum -c -; fi

FROM ghcr.io/huggingface/text-embeddings-inference:cpu-1.10.0@sha256:f0a4bb206b9d4877746b5b9f638e138fe683eeb9787c14d541a12c993b562d86 AS base-amd64
FROM ghcr.io/huggingface/text-embeddings-inference:cpu-arm64-1.10.0@sha256:446c683eef5bbbdf1eb9899966b8e198921f671e92ee3fe3d7a5f6d770ea0a26 AS base-arm64

FROM base-${TARGETARCH}
COPY --from=model /model /model
ENV HF_HUB_OFFLINE=1 HF_HOME=/tmp
USER 10001:10001
EXPOSE 80
CMD ["--model-id", "/model", "--pooling", "mean", "--port", "80", "--max-client-batch-size", "8"]
