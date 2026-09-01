#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cf="$here/../caddy/Caddyfile.airgap"
[ -f "$cf" ] || { echo "FAIL: Caddyfile.airgap missing"; exit 1; }

grep -Eq 'tls[[:space:]]+\{\$TLS_CERT\}[[:space:]]+\{\$TLS_KEY\}' "$cf" \
  || { echo "FAIL: explicit 'tls {\$TLS_CERT} {\$TLS_KEY}' not found"; exit 1; }
grep -q 'reverse_proxy' "$cf" || { echo "FAIL: reverse_proxy block missing"; exit 1; }

# air-gap: no ACME, no email, no internal issuer
grep -qi 'acme'          "$cf" && { echo "FAIL: acme present"; exit 1; }
grep -q  '@'             "$cf" && { echo "FAIL: email address present"; exit 1; }
grep -q  'tls internal'  "$cf" && { echo "FAIL: tls internal present"; exit 1; }
echo "PASS: Caddyfile.airgap uses explicit leaf, no ACME"
