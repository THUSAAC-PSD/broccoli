#!/usr/bin/env bash
# Verify a broccoli air-gap bundle's integrity, then docker-load its images.
# TARGET-SIDE: performs NO network access. Docker images come only from the
# bundle's images/*.tar via `docker load`; downstream compose uses --pull never.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
source "$here/lib/manifest.sh"

usage() { echo "Usage: load-bundle.sh --bundle DIR [--verify-only]"; }

BUNDLE="" VERIFY_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    --verify-only) VERIFY_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$BUNDLE" ] || { echo "--bundle is required" >&2; usage; exit 2; }
[ -f "$BUNDLE/manifest.sha256" ] || { echo "no manifest.sha256 in $BUNDLE" >&2; exit 1; }

echo "verifying bundle integrity ..."
manifest_verify "$BUNDLE" >/dev/null || { echo "ABORT: bundle integrity check failed" >&2; exit 1; }
echo "integrity OK"

[ "$VERIFY_ONLY" = "1" ] && { echo "verify-only: done"; exit 0; }

command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }
for tar in "$BUNDLE"/images/*.tar; do
  [ -e "$tar" ] || { echo "no images/*.tar in bundle" >&2; exit 1; }
  echo "loading $tar ..."
  docker load -i "$tar"
done
echo "all images loaded"
