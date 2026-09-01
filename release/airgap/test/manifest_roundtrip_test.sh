#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
source "$here/../lib/manifest.sh"

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
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

echo "PASS: manifest generate/verify round-trips and rejects tamper"
