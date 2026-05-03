#!/usr/bin/env bash
# Fetches stress-test binaries from GitHub Releases into
# packages/server/embedded/stress-test/, so `cargo build -p server
# --features bundled-stress-test` can include_bytes! them.
#
# Usage: scripts/fetch-stress-test-binaries.sh <version-tag>
# Example: scripts/fetch-stress-test-binaries.sh v0.2.0
#
# Set BROCCOLI_RELEASES_BASE to override the GitHub Releases base URL
# (e.g., to use a mirror). Defaults to upstream GitHub.

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version-tag>  e.g. v0.2.0" >&2
  exit 64
fi

VERSION="$1"
BASE="${BROCCOLI_RELEASES_BASE:-https://github.com/THUSAAC-PSD/broccoli/releases/download}"
OUT_DIR="packages/server/embedded/stress-test"

PLATFORMS=(
  "linux-x86_64"
  "linux-aarch64"
  "windows-x86_64.exe"
  "macos-universal"
)

mkdir -p "$OUT_DIR"

for p in "${PLATFORMS[@]}"; do
  url="$BASE/$VERSION/broccoli-stress-test-$p"
  out="$OUT_DIR/$p"
  echo "fetching $url"
  curl -fSL --retry 3 -o "$out" "$url"

  sha_url="$url.sha256"
  sha_file="$out.sha256"
  curl -fSL --retry 3 -o "$sha_file" "$sha_url"
  pushd "$OUT_DIR" > /dev/null
  if command -v sha256sum > /dev/null; then
    sha256sum -c "$(basename "$sha_file")"
  else
    # macOS fallback
    expected=$(awk '{print $1}' "$(basename "$sha_file")")
    actual=$(shasum -a 256 "$(basename "$out")" | awk '{print $1}')
    if [[ "$expected" != "$actual" ]]; then
      echo "checksum mismatch for $out: expected $expected got $actual" >&2
      exit 1
    fi
    echo "$(basename "$out"): OK"
  fi
  rm -f "$(basename "$sha_file")"
  popd > /dev/null
done

manifest_url="$BASE/$VERSION/manifest.json"
echo "fetching $manifest_url"
curl -fSL --retry 3 -o "$OUT_DIR/manifest.json" "$manifest_url"

echo
echo "done. Binaries staged in $OUT_DIR."
echo "Build with: cargo build -p server --release --features bundled-stress-test"
