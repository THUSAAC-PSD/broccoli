#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
lb="$here/../load-bundle.sh"
[ -f "$lb" ] || { echo "FAIL: load-bundle.sh missing"; exit 1; }
bash -n "$lb" || { echo "FAIL: load-bundle.sh does not parse"; exit 1; }

# offline invariant
grep -Eq '\b(curl|wget|apt|apt-get|pip[0-9]*)\b' "$lb" && { echo "FAIL: network fetch present"; exit 1; }
grep -q 'docker pull' "$lb" && { echo "FAIL: docker pull present"; exit 1; }

# --verify-only path works on a hand-built fixture (no docker needed)
here2="$(cd "$here/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
mkdir -p "$T/images"
printf '{"version":"test"}\n' > "$T/bundle.json"
printf 'fake-image-tar\n' > "$T/images/x.tar"
# shellcheck source=/dev/null
source "$here2/lib/manifest.sh"; manifest_generate "$T"

bash "$lb" --bundle "$T" --verify-only >/dev/null || { echo "FAIL: verify-only rejected a clean bundle"; exit 1; }

printf 'TAMPER\n' >> "$T/bundle.json"
if bash "$lb" --bundle "$T" --verify-only >/dev/null 2>&1; then
  echo "FAIL: verify-only accepted a tampered bundle"; exit 1
fi
echo "PASS: load-bundle verifies manifest, rejects tamper, is offline"
