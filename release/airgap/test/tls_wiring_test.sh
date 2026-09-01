#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
inst="$here/../install.sh"
cf="$here/../caddy/Caddyfile.airgap"
gw="$here/../../docker-compose.gateway-airgap.yaml.template"

# C2: install.sh must NOT envsubst the Caddyfile (that corrupts Caddy {$VAR}).
grep -q 'envsubst' "$inst" && { echo "FAIL: install.sh still runs envsubst (corrupts Caddy {\$VAR})"; exit 1; } || true
# C1: server role must compose the TLS gateway template.
grep -q 'docker-compose.gateway-airgap.yaml.template' "$inst" || { echo "FAIL: server install does not compose the TLS gateway"; exit 1; }
# gateway template exists, publishes 443, mounts the un-rendered Caddyfile.
[ -f "$gw" ] || { echo "FAIL: gateway-airgap compose template missing"; exit 1; }
grep -q ':443' "$gw" || { echo "FAIL: gateway does not publish 443"; exit 1; }
grep -q 'Caddyfile.airgap:/etc/caddy/Caddyfile' "$gw" || { echo "FAIL: gateway does not mount the un-rendered Caddyfile"; exit 1; }
# Caddyfile keeps Caddy-native placeholders (never pre-rendered).
grep -qF 'tls {$TLS_CERT} {$TLS_KEY}' "$cf" || { echo "FAIL: Caddyfile lost its Caddy {\$VAR} tls placeholders"; exit 1; }
echo "PASS: TLS gateway wired; no envsubst; Caddy-native placeholders intact"
