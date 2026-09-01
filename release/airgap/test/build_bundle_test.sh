#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bb="$here/../build-bundle.sh"
[ -f "$bb" ] || { echo "FAIL: build-bundle.sh missing"; exit 1; }
bash -n "$bb" || { echo "FAIL: build-bundle.sh does not parse"; exit 1; }

# structural: mints CA and writes manifest + bundle.json
grep -q 'mint-ca.sh'      "$bb" || { echo "FAIL: does not mint CA"; exit 1; }
grep -q 'manifest_generate' "$bb" || { echo "FAIL: does not generate manifest"; exit 1; }
grep -q 'bundle.json'     "$bb" || { echo "FAIL: does not write bundle.json"; exit 1; }

# --skip-images assembles a tree with no docker (CI-safe)
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
bash "$bb" --version testv --output "$T" --skip-images >/dev/null
b="$T/broccoli-airgap-testv"
for p in bundle.json manifest.sha256 ca/root.crt caddy/Caddyfile.airgap \
         load-bundle.sh install.sh trust-ca/linux.sh compose lib/manifest.sh \
         native/live-boot-preflight.sh; do
  [ -e "$b/$p" ] || { echo "FAIL: bundle missing $p"; exit 1; }
done
# manifest actually verifies
# shellcheck source=/dev/null
source "$here/../lib/manifest.sh"; manifest_verify "$b" >/dev/null \
  || { echo "FAIL: assembled bundle fails its own manifest"; exit 1; }
echo "PASS: build-bundle assembles a verifiable tree (skip-images)"
