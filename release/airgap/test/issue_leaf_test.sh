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

perms="$(stat -c '%a' "$LEAF/server.key")"
[ "$perms" = "600" ] || { echo "FAIL: server.key perms $perms != 600"; exit 1; }

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

# the leaf out dir is created 0700 so server.key is never momentarily exposed to
# group/world between mkdir and its 0600 chmod (default umask 022 -> 0755 parent).
sub="$LEAF/nested-leaf"
bash "$issue" --ca-dir "$CA" --host judge.contest.lan --out "$sub" --days 30 >/dev/null
dperm="$(stat -c '%a' "$sub")"
[ "$dperm" = "700" ] || { echo "FAIL: leaf out dir perms $dperm != 700"; exit 1; }

# input validation: a --host must be a bare DNS name or IP literal. A comma would
# splice an extra SAN entry and a newline would inject an extfile directive (e.g.
# basicConstraints=CA:TRUE, forging a CA-capable leaf) — both refused before openssl.
if bash "$issue" --ca-dir "$CA" --host 'a,IP:10.0.0.9' --out "$LEAF/inj-comma" --days 30 >/dev/null 2>&1; then
  echo "FAIL: issue-leaf accepted a --host with a comma (SAN injection)"; exit 1; fi
[ -e "$LEAF/inj-comma/server.crt" ] && { echo "FAIL: issue-leaf wrote a cert for a comma --host"; exit 1; } || true
if bash "$issue" --ca-dir "$CA" --host "$(printf 'good.lan\nsubjectKeyIdentifier=hash')" --out "$LEAF/inj-nl" --days 30 >/dev/null 2>&1; then
  echo "FAIL: issue-leaf accepted a --host with a newline (extfile injection)"; exit 1; fi
[ -e "$LEAF/inj-nl/server.crt" ] && { echo "FAIL: issue-leaf wrote a cert for a newline --host"; exit 1; } || true
# a legitimate extra DNS label still issues fine
bash "$issue" --ca-dir "$CA" --host node-1.contest.lan --out "$LEAF/ok-extra" --days 30 >/dev/null \
  || { echo "FAIL: issue-leaf rejected a legitimate hyphenated DNS host"; exit 1; }

echo "PASS: issue-leaf chains to CA with DNS+IP SANs and serverAuth"
