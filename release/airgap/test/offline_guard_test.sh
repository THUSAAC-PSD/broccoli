#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/.."

# Target-side scripts that run on the air-gapped LAN. build-bundle.sh is
# STAGING-side and intentionally excluded.
targets=(
  "$root/load-bundle.sh"
  "$root/install.sh"
  "$root/ca/issue-leaf.sh"
  "$root/trust-ca/linux.sh"
  "$root/trust-ca/macos.sh"
)
[ "${#targets[@]}" -gt 0 ] || { echo "FAIL: empty target list"; exit 1; }

rc=0
for f in "${targets[@]}"; do
  [ -f "$f" ] || { echo "FAIL: missing $f"; rc=1; continue; }
  if grep -Eq '\b(curl|wget|apt|apt-get|pip[0-9]*)\b' "$f"; then
    echo "FAIL: $f contains a network fetch"; rc=1
  fi
  # docker pull is only allowed with --pull never
  if grep -En 'docker[[:space:]]+pull' "$f" >/dev/null; then
    echo "FAIL: $f uses 'docker pull'"; rc=1
  fi
done
[ "$rc" = "0" ] && echo "PASS: no target-side script fetches from the network"
exit $rc
