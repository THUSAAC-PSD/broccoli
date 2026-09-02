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
# fail-closed offline: auto_https fully OFF. `disable_redirects` alone leaves the
# ACME/cert-management machinery running — with no internet it churns on failed
# issuance and can fall back to serving an on-the-fly self-signed cert for names
# we never minted. `off` disables all of it; we serve only the explicit leaf.
grep -Eq '^[[:space:]]*auto_https[[:space:]]+off([[:space:]]|$)' "$cf" \
  || { echo "FAIL: auto_https not set to 'off' (fail-closed offline)"; exit 1; }
grep -q 'disable_redirects' "$cf" \
  && { echo "FAIL: 'auto_https disable_redirects' still present (leaves cert automation on)"; exit 1; } || true
echo "PASS: Caddyfile.airgap uses explicit leaf, no ACME"
