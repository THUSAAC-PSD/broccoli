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

# --- portable sha256: a bundle manifested with coreutils `sha256sum` must still
#     verify on a host that has ONLY `shasum -a 256` (macOS operators cross-check
#     a Linux-built bundle). Build a curated PATH that hides sha256sum, exposes
#     shasum plus the coreutils the lib needs, and re-source the lib there so its
#     tool resolver falls back to shasum. Both must agree byte-for-byte. ---
if command -v shasum >/dev/null 2>&1; then
  bin="$(mktemp -d)"; X="$(mktemp -d)"
  trap 'rm -rf "$T" "$gen" "$bin" "$X"' EXIT
  for t in find sort xargs mktemp diff rm; do ln -s "$(command -v "$t")" "$bin/$t"; done
  ln -s "$(command -v shasum)" "$bin/shasum"          # shasum present; sha256sum absent from $bin
  bash_abs="$(command -v bash)"
  printf 'gamma\n' > "$X/g.txt"; mkdir -p "$X/d"; printf 'delta\n' > "$X/d/e.txt"
  manifest_generate "$X"                              # generated with default tool (sha256sum here)
  PATH="$bin" "$bash_abs" -c '
      set -euo pipefail
      source "'"$here"'/../lib/manifest.sh"
      command -v sha256sum >/dev/null 2>&1 && { echo "sha256sum still on PATH — test not exercising fallback"; exit 3; }
      [ "$(manifest_verify "'"$X"'")" = OK ]' \
    || { echo "FAIL: sha256sum-built manifest did not verify under a shasum-only PATH"; exit 1; }
  # reverse: a manifest generated under shasum-only must verify back under sha256sum
  PATH="$bin" "$bash_abs" -c '
      set -euo pipefail
      source "'"$here"'/../lib/manifest.sh"
      manifest_generate "'"$X"'"'
  [ "$(manifest_verify "$X")" = OK ] \
    || { echo "FAIL: shasum-built manifest did not verify back under sha256sum"; exit 1; }
  echo "PASS: manifest sha256 is portable across sha256sum and shasum -a 256"
else
  echo "SKIP: shasum unavailable — portable-sha256 cross-check skipped"
fi

echo "PASS: manifest generate/verify round-trips and rejects tamper"
