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
mkdir -p "$OUT"

umask 077
openssl ecparam -name prime256v1 -genkey -noout -out "$OUT/root.key"
chmod 0600 "$OUT/root.key"

openssl req -x509 -new -key "$OUT/root.key" -sha256 -days "$DAYS" \
  -out "$OUT/root.crt" \
  -subj "/O=${ORG}/CN=${CN}" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"

echo "minted CA: $OUT/root.crt (key: $OUT/root.key, mode 0600)"
