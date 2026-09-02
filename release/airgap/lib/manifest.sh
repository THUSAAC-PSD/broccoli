#!/usr/bin/env bash
# Manifest generate/verify shared by build-bundle.sh (staging) and
# load-bundle.sh (target). sha256 over every bundle file except the
# manifest itself; paths are relative to the bundle root and sorted so
# the manifest is reproducible.
set -euo pipefail

# Portable sha256: coreutils `sha256sum` (Linux) or `shasum -a 256` (macOS). Both
# emit identical "<hex>  <path>" lines, so a manifest written with one
# cross-verifies under the other — a macOS operator can check a Linux-built
# bundle. Kept as an array so it expands into xargs as the command plus its args.
# Air-gap safe: both are base-OS tools, no network.
if command -v sha256sum >/dev/null 2>&1; then
  _MANIFEST_SHA256=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  _MANIFEST_SHA256=(shasum -a 256)
else
  echo "manifest: need sha256sum or shasum -a 256 (neither found)" >&2
  return 1 2>/dev/null || exit 1
fi

# NOTE: .env.infra/.env.server/.env.worker are on-host runtime config generated
# on the server/worker (they never crossed the air gap and hold secrets), so
# they are excluded from transport-integrity. *.example templates ARE covered.
manifest_generate() {
  local dir="$1"
  ( cd "$dir"
    find . -type f ! -name manifest.sha256 ! -name .env.infra ! -name .env.server ! -name .env.worker -print0 \
      | LC_ALL=C sort -z \
      | xargs -0 "${_MANIFEST_SHA256[@]}" \
      > manifest.sha256
  )
}

manifest_verify() {
  local dir="$1"
  [ -f "$dir/manifest.sha256" ] || { echo "manifest.sha256 missing" >&2; return 1; }
  local tmp; tmp="$(mktemp)"
  ( cd "$dir"
    find . -type f ! -name manifest.sha256 ! -name .env.infra ! -name .env.server ! -name .env.worker -print0 \
      | LC_ALL=C sort -z \
      | xargs -0 "${_MANIFEST_SHA256[@]}"
  ) > "$tmp" 2>/dev/null
  if diff -q "$tmp" "$dir/manifest.sha256" >/dev/null 2>&1; then
    rm -f "$tmp"; echo OK; return 0
  else
    rm -f "$tmp"; echo "manifest verification failed" >&2; return 1
  fi
}
