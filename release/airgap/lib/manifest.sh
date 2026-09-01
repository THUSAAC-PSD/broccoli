#!/usr/bin/env bash
# Manifest generate/verify shared by build-bundle.sh (staging) and
# load-bundle.sh (target). sha256 over every bundle file except the
# manifest itself; paths are relative to the bundle root and sorted so
# the manifest is reproducible.
set -euo pipefail

manifest_generate() {
  local dir="$1"
  ( cd "$dir"
    find . -type f ! -name manifest.sha256 -print0 \
      | LC_ALL=C sort -z \
      | xargs -0 sha256sum \
      > manifest.sha256
  )
}

manifest_verify() {
  local dir="$1"
  [ -f "$dir/manifest.sha256" ] || { echo "manifest.sha256 missing" >&2; return 1; }
  ( cd "$dir" && sha256sum -c --strict --quiet manifest.sha256 ) \
    && { echo OK; return 0; } || { echo "manifest verification failed" >&2; return 1; }
}
