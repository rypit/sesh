# The agent's browser (specs/010, ADR 0013): Chromium, driven by the
# gateway over CDP, online only through the gateway's egress proxy.
#
#   docker build -f deploy/browser.Dockerfile -t sesh-browser:dev .
#
# The container is untrusted: it holds no secrets (the gateway fills
# logins), and its only network reaches the gateway.
FROM debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e

RUN apt-get update \
 && apt-get install -y --no-install-recommends chromium fonts-liberation fonts-noto-color-emoji ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 10001 --create-home --home-dir /home/browser browser

COPY deploy/browser/start.sh /usr/local/bin/sesh-browser-start
RUN chmod 0755 /usr/local/bin/sesh-browser-start

USER 10001:10001
# CDP, on the container's own address (its only network is the gateway's).
EXPOSE 9222
ENTRYPOINT ["/usr/local/bin/sesh-browser-start"]
