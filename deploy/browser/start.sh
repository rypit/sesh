#!/bin/sh
# Starts Chromium for one agent (specs/010 research.md R3-R5).
#   SESH_PROXY    the gateway's egress proxy, e.g. http://gateway-browser-ana:7004
#   SESH_PROFILE  the profile directory (a per-agent volume)
set -eu
: "${SESH_PROXY:?}"
PROFILE="${SESH_PROFILE:-/profile}"
ADDR="$(hostname -i | awk '{print $1}')"
# New headless Chromium listens for CDP on loopback only (it ignores
# --remote-debugging-address), so socat exposes it on the container's
# address, which is only on the network shared with the gateway.
socat "TCP-LISTEN:9222,bind=$ADDR,fork,reuseaddr" TCP:127.0.0.1:9223 &
exec chromium \
  --headless=new \
  --remote-debugging-port=9223 \
  --remote-allow-origins=* \
  --user-data-dir="$PROFILE" \
  --proxy-server="$SESH_PROXY" \
  --proxy-bypass-list="<-loopback>" \
  --disable-background-networking \
  --disable-component-update \
  --disable-default-apps \
  --disable-domain-reliability \
  --disable-sync \
  --no-first-run \
  --no-default-browser-check \
  --metrics-recording-only \
  --password-store=basic \
  --disable-features=Translate,MediaRouter,OptimizationHints,AutofillServerCommunication \
  --force-webrtc-ip-handling-policy=disable_non_proxied_udp \
  --window-size=1280,800 \
  ${SESH_BROWSER_FLAGS:-} \
  about:blank
