#!/usr/bin/env bash
# Assemble a single self-contained broccoli air-gap bundle on a networked
# staging box. STAGING-SIDE: may pull/build/docker-save. The produced tree
# is carried to the air-gapped LAN and installed with zero network.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
# shellcheck source=/dev/null
source "$here/lib/manifest.sh"

usage() {
  echo "Usage: build-bundle.sh --version V [--output DIR] [--lan-host H] [--tar] [--skip-images]"
}

VERSION="" OUTPUT="./dist" LAN_HOST="" TAR=0 SKIP_IMAGES=0
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --output) OUTPUT="$2"; shift 2 ;;
    --lan-host) LAN_HOST="$2"; shift 2 ;;
    --tar) TAR=1; shift ;;
    --skip-images) SKIP_IMAGES=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$VERSION" ] || { echo "--version is required" >&2; usage; exit 2; }
case "$VERSION" in
  *[!A-Za-z0-9._-]*) echo "--version must match [A-Za-z0-9._-] (got: $VERSION)" >&2; exit 2 ;;
esac

B="$OUTPUT/broccoli-airgap-$VERSION"
rm -rf "$B"
mkdir -p "$B"/{images,compose,cli,ca,caddy,trust-ca,lib,native}

# 1. CA (bundle-time mint; root.key stays 0600, server-only)
bash "$here/ca/mint-ca.sh" --out "$B/ca"
if [ -n "$LAN_HOST" ]; then
  bash "$here/ca/issue-leaf.sh" --ca-dir "$B/ca" --host "$LAN_HOST" --out "$B/ca"
fi

# 2. Target-side scripts + Caddyfile + trust helpers + manifest lib
cp "$here/load-bundle.sh" "$here/install.sh" "$B/"
cp "$here/lib/manifest.sh" "$B/lib/manifest.sh"
cp "$here/caddy/Caddyfile.airgap" "$B/caddy/Caddyfile.airgap"
cp "$here"/ca/issue-leaf.sh "$B/ca/issue-leaf.sh"
cp "$here"/trust-ca/* "$B/trust-ca/"
cp "$repo/release/native/live-boot-preflight.sh" "$B/native/live-boot-preflight.sh"
chmod +x "$B/native/live-boot-preflight.sh"

# 3. Compose templates + env examples (reuse release/, do not fork)
cp "$repo/release/docker-compose.server.yaml.template" \
   "$repo/release/docker-compose.infra.yaml.template" "$B/compose/"
cp "$repo/release/.env.server.example" "$repo/release/.env.infra.example" "$B/compose/"

# 4. Images + CLI (heavy; skipped for CI structural tests)
if [ "$SKIP_IMAGES" = "0" ]; then
  # Frontend baked fresh into the server image; verify served bundle
  # behaviorally rather than trusting mtime (e2e-frontend-deploy-staleness).
  # server + infra + worker images are built/pulled then docker-saved:
  #   docker build ... -t broccoli-server:$VERSION .
  #   docker save broccoli-server:$VERSION postgres:... redis:... > images/*.tar
  #   docker build -f Dockerfile.worker --target runtime-full -t broccoli-worker:$VERSION .
  #   docker save broccoli-worker:$VERSION > images/worker.tar
  # CLI (musl-static, per noi-parity-worker-image):
  #   cargo build -p broccoli-contestant-cli --profile release-cli \
  #     --target x86_64-unknown-linux-musl && cp target/.../broccoli cli/broccoli
  echo "NOTE: image/CLI assembly runs here on the staging box (see comments)"
fi

# 5. Provenance + integrity
git_sha="$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)"
cat > "$B/bundle.json" <<JSON
{
  "version": "$VERSION",
  "git_sha": "$git_sha",
  "roles": ["server", "worker", "contestant"]
}
JSON
manifest_generate "$B"
echo "assembled bundle: $B"

if [ "$TAR" = "1" ]; then
  ( cd "$OUTPUT" && tar -caf "broccoli-airgap-$VERSION.tar.zst" "broccoli-airgap-$VERSION" )
  echo "tarball: $OUTPUT/broccoli-airgap-$VERSION.tar.zst"
fi
