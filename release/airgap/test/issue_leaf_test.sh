#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
mint="$here/../ca/mint-ca.sh"
issue="$here/../ca/issue-leaf.sh"

CA="$(mktemp -d)"; LEAF="$(mktemp -d)"; trap 'rm -rf "$CA" "$LEAF"' EXIT
bash "$mint" --out "$CA" --days 30
bash "$issue" --ca-dir "$CA" --host judge.contest.lan --host 10.0.0.5 --out "$LEAF" --days 30

[ -f "$LEAF/server.crt" ] || { echo "FAIL: server.crt missing"; exit 1; }
[ -f "$LEAF/server.key" ] || { echo "FAIL: server.key missing"; exit 1; }

# chains to the CA
openssl verify -CAfile "$CA/root.crt" "$LEAF/server.crt" >/dev/null \
  || { echo "FAIL: leaf does not chain to CA"; exit 1; }

# SAN carries both the DNS name and the IP literal
san="$(openssl x509 -in "$LEAF/server.crt" -noout -ext subjectAltName)"
echo "$san" | grep -q 'DNS:judge.contest.lan' || { echo "FAIL: DNS SAN missing"; exit 1; }
echo "$san" | grep -q 'IP Address:10.0.0.5'   || { echo "FAIL: IP SAN missing"; exit 1; }

# serverAuth EKU present
openssl x509 -in "$LEAF/server.crt" -noout -ext extendedKeyUsage | grep -q 'TLS Web Server Authentication' \
  || { echo "FAIL: serverAuth EKU missing"; exit 1; }
echo "PASS: issue-leaf chains to CA with DNS+IP SANs and serverAuth"
