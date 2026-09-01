#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
mint="$here/../ca/mint-ca.sh"

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
bash "$mint" --out "$T" --days 30

[ -f "$T/root.crt" ] || { echo "FAIL: root.crt missing"; exit 1; }
[ -f "$T/root.key" ] || { echo "FAIL: root.key missing"; exit 1; }

perms="$(stat -c '%a' "$T/root.key")"
[ "$perms" = "600" ] || { echo "FAIL: root.key perms $perms != 600"; exit 1; }

txt="$(openssl x509 -in "$T/root.crt" -noout -text)"
echo "$txt" | grep -q 'CA:TRUE'                    || { echo "FAIL: root.crt not a CA cert"; exit 1; }
echo "$txt" | grep -q 'Basic Constraints: critical' || { echo "FAIL: basicConstraints not critical"; exit 1; }
echo "$txt" | grep -q 'Key Usage: critical'         || { echo "FAIL: keyUsage not critical"; exit 1; }
echo "$txt" | grep -q 'Certificate Sign'            || { echo "FAIL: keyCertSign missing"; exit 1; }
echo "$txt" | grep -q 'NIST CURVE: P-256'           || { echo "FAIL: key is not ECDSA P-256"; exit 1; }
echo "PASS: mint-ca produces a CA cert + 0600 key"
