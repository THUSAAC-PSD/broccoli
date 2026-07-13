#!/usr/bin/env bash
# Stage the C/C++/Python toolchain (build-essential + python3 closure) as offline
# .deb sets for live-boot / airgapped Broccoli WORKER boxes. install-native.sh
# (worker role) auto-installs the set matching the box's Ubuntu VERSION_ID via
# `dpkg -i toolchain/<ver>/*.deb` before falling back to apt.
#
# Run on ANY machine with Docker + internet (it does NOT need to match the target
# Ubuntu version -- each set is built inside that version's container):
#   ./stage-toolchain.sh                 # stages 20.04 22.04 24.04 (default)
#   ./stage-toolchain.sh 22.04           # just one version
# Output: release/native/toolchain/<ver>/*.deb -- folded into the contest bundle by
# build-native-bundle.sh. Re-run any time to refresh.
#
# NO DOCKER? On an online box that IS the target version, capture it natively:
#   sudo apt-get update
#   sudo apt-get install -y --download-only build-essential python3
#   v=$(. /etc/os-release; echo "$VERSION_ID"); mkdir -p "toolchain/$v"
#   cp /var/cache/apt/archives/*.deb "toolchain/$v/"
#
# Note: the closure is resolved against a MINIMAL base image, so it is a superset
# of what a full Desktop live ISO needs (those extra libs dpkg-skip as already
# installed). Within one LTS this is robust; across a major point-release skew the
# self-install path (apt / LAN mirror) remains the always-correct fallback.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERS=("$@"); [ ${#VERS[@]} -gt 0 ] || VERS=(20.04 22.04 24.04)

command -v docker >/dev/null 2>&1 || {
  echo "error: docker not found -- see the NO DOCKER fallback in this script's header." >&2
  exit 1
}

for ver in "${VERS[@]}"; do
  dest="$HERE/toolchain/$ver"
  mkdir -p "$dest"
  echo ">> staging toolchain for Ubuntu $ver -> $dest"
  rm -f "$dest"/*.deb
  # --platform pins amd64: the whole bundle (isolate/server/worker) is x86_64, so
  # the toolchain MUST be amd64 even when staging from an arm64 host (qemu-emulated).
  docker run --rm --platform linux/amd64 -v "$dest:/out" "ubuntu:$ver" bash -c '
    set -e
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y --download-only --no-install-recommends build-essential python3
    cp /var/cache/apt/archives/*.deb /out/ 2>/dev/null || true
    chmod 644 /out/*.deb 2>/dev/null || true
  ' || { echo "  WARN: staging $ver failed (image pull or apt). skipping." >&2; continue; }
  n=$(ls "$dest"/*.deb 2>/dev/null | wc -l | tr -d ' ')
  if [ "$n" = 0 ]; then
    echo "  WARN: no .debs captured for $ver" >&2
  else
    echo "  staged $ver: $n debs ($(du -sh "$dest" | cut -f1))"
  fi
done

echo ">> done. toolchain sets under $HERE/toolchain/ ; rebuild the bundle to include them."
