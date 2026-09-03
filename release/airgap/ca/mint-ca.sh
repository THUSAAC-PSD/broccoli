#!/usr/bin/env bash
# Mint an internal root CA for an air-gapped broccoli LAN. Runs on the
# networked staging box at bundle-assembly time. root.crt ships to every
# machine (public); root.key is the signing secret (0600, server-only).
set -euo pipefail

usage() { echo "Usage: mint-ca.sh --out DIR [--org NAME] [--cn NAME] [--days N]"; }

OUT="" ORG="Broccoli LAN" CN="Broccoli LAN Root CA" DAYS=120
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --org) ORG="$2"; shift 2 ;;
    --cn)  CN="$2";  shift 2 ;;
    --days) DAYS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { echo "--out is required" >&2; usage; exit 2; }

# Reject subject DN components that could inject extra RDNs. '/' is the -subj
# field separator and a newline would splice new DN lines — either could forge
# O=/CN= fields. `case` (not grep) so an embedded newline is matched rather than
# swallowed as a line separator.
for v in "$ORG" "$CN"; do
  case "$v" in
    ""|*[[:cntrl:]]*|*/*)
      echo "invalid --org/--cn value: must be non-empty with no control chars or '/'" >&2; exit 2 ;;
  esac
done

mkdir -p "$OUT"
# Lock the dir down BEFORE writing root.key: default umask 022 leaves a fresh
# dir 0755, so the private key would be briefly group/world-readable.
chmod 700 "$OUT"

umask 077
openssl ecparam -name prime256v1 -genkey -noout -out "$OUT/root.key"
chmod 0600 "$OUT/root.key"

openssl req -x509 -new -key "$OUT/root.key" -sha256 -days "$DAYS" \
  -out "$OUT/root.crt" \
  -subj "/O=${ORG}/CN=${CN}" \
  -addext "basicConstraints=critical,CA:TRUE,pathlen:0" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"

echo "minted CA: $OUT/root.crt (key: $OUT/root.key, mode 0600)"
