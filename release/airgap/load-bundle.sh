#!/usr/bin/env bash
# Verify a broccoli air-gap bundle's integrity, then load its images via the
# container engine. TARGET-SIDE: performs NO network access. Images come only
# from the bundle's images/*.tar via `<engine> load`; downstream compose uses
# --pull never.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
source "$here/lib/manifest.sh"
# shellcheck source=/dev/null
source "$here/lib/runtime.sh"

usage() { echo "Usage: load-bundle.sh --bundle DIR [--verify-only] [--pristine]"; }

BUNDLE="" VERIFY_ONLY=0 PRISTINE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    --verify-only) VERIFY_ONLY=1; shift ;;
    --pristine) PRISTINE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$BUNDLE" ] || { echo "--bundle is required" >&2; usage; exit 2; }
[ -f "$BUNDLE/manifest.sha256" ] || { echo "no manifest.sha256 in $BUNDLE" >&2; exit 1; }

echo "verifying bundle integrity ..."
manifest_verify "$BUNDLE" >/dev/null || { echo "ABORT: bundle integrity check failed" >&2; exit 1; }
echo "integrity OK"

# --pristine: the on-host env files are excluded from the manifest, so a planted
# one passes integrity undetected. A freshly-transported bundle (contestant, or
# a pre-generation deploy) must carry NONE of them — refuse if one is present.
if [ "$PRISTINE" = "1" ]; then
  manifest_no_hostenv "$BUNDLE" \
    || { echo "ABORT: bundle carries on-host env config on a supposedly pristine copy (possible tampering) — rebuild from trusted media; see release/docs/airgap-deployment.md" >&2; exit 1; }
  echo "pristine OK (no on-host env config present)"
fi

[ "$VERIFY_ONLY" = "1" ] && { echo "verify-only: done"; exit 0; }

# Honor an explicit BROCCOLI_ENGINE (setup.sh exports it); else self-detect a
# working engine — install.sh calls us before it resolves the engine, and both
# docker and podman support `load -i`.
engine="${BROCCOLI_ENGINE:-$(runtime_engine)}"
[ -n "$engine" ] || { echo "no working docker or podman found" >&2; exit 1; }
command -v "$engine" >/dev/null 2>&1 || { echo "$engine not found" >&2; exit 1; }
for tar in "$BUNDLE"/images/*.tar; do
  [ -e "$tar" ] || { echo "no images/*.tar in bundle" >&2; exit 1; }
  echo "loading $tar ..."
  "$engine" load -i "$tar"
done
echo "all images loaded"
