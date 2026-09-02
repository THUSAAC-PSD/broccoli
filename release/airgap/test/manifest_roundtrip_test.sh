#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
source "$here/../lib/manifest.sh"

T="$(mktemp -d)"
gen="$(mktemp -d)"
trap 'rm -rf "$T" "$gen"' EXIT
mkdir -p "$T/sub"
printf 'alpha\n' > "$T/a.txt"
printf 'beta\n'  > "$T/sub/b.txt"

manifest_generate "$T"
[ -f "$T/manifest.sha256" ] || { echo "FAIL: manifest not written"; exit 1; }
grep -q 'a.txt' "$T/manifest.sha256" || { echo "FAIL: a.txt missing from manifest"; exit 1; }
grep -q 'manifest.sha256' "$T/manifest.sha256" && { echo "FAIL: manifest lists itself"; exit 1; }

manifest_verify "$T" >/dev/null || { echo "FAIL: clean tree failed verify"; exit 1; }

# Tamper one byte -> verify MUST reject
printf 'TAMPERED\n' > "$T/a.txt"
if manifest_verify "$T" >/dev/null 2>&1; then
  echo "FAIL: tampered tree passed verify"; exit 1
fi

manifest_generate "$T"
manifest_verify "$T" >/dev/null || { echo "FAIL: regenerated clean tree failed verify"; exit 1; }
printf 'sneaky\n' > "$T/added.txt"
if manifest_verify "$T" >/dev/null 2>&1; then
  echo "FAIL: added file passed verify"; exit 1
fi

# --- generated on-host runtime config must not break integrity ---
mkdir -p "$gen/compose"
echo "img" > "$gen/images.txt"
printf '.env.infra.example\n' > "$gen/compose/.env.infra.example"
printf '.env.server.example\n' > "$gen/compose/.env.server.example"
printf '.env.worker.example\n' > "$gen/compose/.env.worker.example"
manifest_generate "$gen"
# operator/generated env files land AFTER the bundle was manifested
printf 'POSTGRES_PASSWORD=secret\n' > "$gen/compose/.env.infra"
printf 'BROCCOLI__DATABASE__URL=x\n' > "$gen/compose/.env.server"
printf 'BROCCOLI__DATABASE__URL=x\n' > "$gen/compose/.env.worker"
[ "$(manifest_verify "$gen")" = OK ] \
  || { echo "FAIL: generated .env.infra/.env.server/.env.worker must be excluded from manifest_verify"; exit 1; }
# .env.infra.example/.env.server.example templates must remain covered
echo tampered >> "$gen/compose/.env.infra.example"
manifest_verify "$gen" >/dev/null 2>&1 \
  && { echo "FAIL: manifest_verify must still catch tampering of .env.infra.example"; exit 1; } || true
# regenerate and tamper .env.server.example
manifest_generate "$gen"
echo tampered >> "$gen/compose/.env.server.example"
manifest_verify "$gen" >/dev/null 2>&1 \
  && { echo "FAIL: manifest_verify must still catch tampering of .env.server.example"; exit 1; } || true
# regenerate and tamper .env.worker.example
manifest_generate "$gen"
echo tampered >> "$gen/compose/.env.worker.example"
manifest_verify "$gen" >/dev/null 2>&1 \
  && { echo "FAIL: manifest_verify must still catch tampering of .env.worker.example"; exit 1; } || true
# tampering with a real bundle file is still caught
manifest_generate "$gen"
echo tampered >> "$gen/images.txt"
manifest_verify "$gen" >/dev/null 2>&1 \
  && { echo "FAIL: manifest_verify must still catch tampering of shipped files"; exit 1; } || true
echo "PASS: manifest excludes on-host env config, still catches tampering"

echo "PASS: manifest generate/verify round-trips and rejects tamper"
