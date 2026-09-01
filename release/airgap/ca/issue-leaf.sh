#!/usr/bin/env bash
# Issue the server TLS leaf for the air-gapped LAN host. Runs on the
# target (install time) OR on staging when --lan-host is known early.
# Each --host becomes an IP: SAN if it parses as an IP literal, else DNS:.
# TARGET-SIDE: no network access.
set -euo pipefail

usage() { echo "Usage: issue-leaf.sh --ca-dir DIR --host H [--host H2 ...] --out DIR [--days N]"; }

CA_DIR="" OUT="" DAYS=120
HOSTS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --ca-dir) CA_DIR="$2"; shift 2 ;;
    --host)   HOSTS+=("$2"); shift 2 ;;
    --out)    OUT="$2"; shift 2 ;;
    --days)   DAYS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$CA_DIR" ] && [ -n "$OUT" ] && [ "${#HOSTS[@]}" -gt 0 ] \
  || { echo "--ca-dir, --out, and at least one --host are required" >&2; usage; exit 2; }
[ -f "$CA_DIR/root.crt" ] && [ -f "$CA_DIR/root.key" ] \
  || { echo "CA dir must contain root.crt and root.key" >&2; exit 2; }
mkdir -p "$OUT"

is_ip() { printf '%s' "$1" | grep -Eq '^([0-9]{1,3}\.){3}[0-9]{1,3}$|:'; }

# Build SAN list
san=""
for h in "${HOSTS[@]}"; do
  if is_ip "$h"; then san="${san}${san:+,}IP:${h}"; else san="${san}${san:+,}DNS:${h}"; fi
done

umask 077
openssl ecparam -name prime256v1 -genkey -noout -out "$OUT/server.key"
chmod 0600 "$OUT/server.key"

# CN = first host for legacy display; SANs are authoritative.
openssl req -new -key "$OUT/server.key" -subj "/CN=${HOSTS[0]}" -out "$OUT/server.csr"

ext="$(mktemp)"; trap 'rm -f "$ext" "$OUT/server.csr"' EXIT
cat > "$ext" <<EXT
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=${san}
EXT

openssl x509 -req -in "$OUT/server.csr" \
  -CA "$CA_DIR/root.crt" -CAkey "$CA_DIR/root.key" -CAcreateserial \
  -sha256 -days "$DAYS" -extfile "$ext" -out "$OUT/server.crt"

echo "issued leaf: $OUT/server.crt (SAN: ${san})"
