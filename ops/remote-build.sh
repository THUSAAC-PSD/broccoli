#!/usr/bin/env bash
# Runs on broccoli-build. Builds linux/amd64 server+worker docker images,
# WASM plugins, and the stress-test binary, then writes the dist bundle.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

TAG="${TAG:-profile2}"
SRC="/root/broccoli"
DIST="/root/broccoli-dist"
LOG_DIR="${DIST}/build-logs"
mkdir -p "${LOG_DIR}" "${DIST}/images" "${DIST}/plugins" "${DIST}/stress-test"

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "${LOG_DIR}/build.log"; }

cd "${SRC}"

# --- Toolchain ----
if ! command -v rustup >/dev/null 2>&1; then
  log "installing rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --no-modify-path
fi
export PATH="$HOME/.cargo/bin:$PATH"
log "syncing toolchain from rust-toolchain.toml..."
rustup show >/dev/null
rustup target add wasm32-wasip1 x86_64-unknown-linux-musl >/dev/null
apt-get install -y -qq musl-tools libssl-dev pkg-config >/dev/null

# --- 1. Server + worker docker images ---
JOBS="$(nproc)"
log "BUILD: broccoli-server:${TAG} (jobs=${JOBS})..."
DOCKER_BUILDKIT=1 docker build -t "broccoli-server:${TAG}" \
  --build-arg CARGO_BUILD_JOBS="${JOBS}" \
  -f Dockerfile.server . 2>&1 | tee "${LOG_DIR}/docker-server.log" | tail -20

log "BUILD: broccoli-worker:${TAG}-icpc (jobs=${JOBS})..."
DOCKER_BUILDKIT=1 docker build -t "broccoli-worker:${TAG}-icpc" \
  --build-arg CARGO_BUILD_JOBS="${JOBS}" \
  -f Dockerfile.worker . 2>&1 | tee "${LOG_DIR}/docker-worker.log" | tail -20

log "saving image tars..."
docker save "broccoli-server:${TAG}"     | gzip -3 > "${DIST}/images/server.tar.gz"
docker save "broccoli-worker:${TAG}-icpc" | gzip -3 > "${DIST}/images/worker-icpc.tar.gz"

# --- 2. Support images (postgres, redis, seaweed, caddy) — pull + save ---
for pair in "postgres:18-alpine|postgres" \
            "redis:7-alpine|redis" \
            "chrislusf/seaweedfs:4.15|seaweedfs" \
            "caddy:2-alpine|caddy"; do
  full="${pair%|*}"; dest="${pair##*|}"
  log "pull+save ${full}..."
  docker pull --platform linux/amd64 "${full}" >/dev/null
  docker save "${full}" | gzip -3 > "${DIST}/images/${dest}.tar.gz"
done

# --- 3. Build all WASM plugins directly (skip pnpm web SDK build) ---
log "BUILD: WASM plugins..."
for d in plugins/*/; do
  name="$(basename "$d")"
  if [[ ! -f "${d}Cargo.toml" || ! -f "${d}plugin.toml" ]]; then continue; fi
  log "  plugin: ${name}"
  ( cd "$d" && cargo build --target wasm32-wasip1 --release 2>&1 | tail -3 ) \
    >>"${LOG_DIR}/plugins.log" 2>&1
  # Copy the produced wasm + manifest into dist.
  mkdir -p "${DIST}/plugins/${name}"
  cp "${d}plugin.toml" "${DIST}/plugins/${name}/"
  wasm="$(find "${d}target/wasm32-wasip1/release/" -maxdepth 1 -name '*.wasm' ! -name 'deps' | head -1)"
  if [[ -n "$wasm" ]]; then
    cp "$wasm" "${DIST}/plugins/${name}/"
    # Also install into source tree so the manifest entry path resolves (some plugins
    # reference name.wasm relative to the plugin dir).
    entry="$(awk -F'"' '/^entry[[:space:]]*=/ {print $2; exit}' "${d}plugin.toml" 2>/dev/null || true)"
    if [[ -n "$entry" ]]; then
      cp "$wasm" "${d}${entry}" 2>/dev/null || true
      cp "$wasm" "${DIST}/plugins/${name}/${entry}" 2>/dev/null || true
    fi
  fi
done

# --- 4. stress-test linux/amd64 musl static binary ---
log "BUILD: stress-test (musl)..."
cargo build -p stress-test --release --target x86_64-unknown-linux-musl 2>&1 | tee "${LOG_DIR}/stress-test.log" | tail -10
cp target/x86_64-unknown-linux-musl/release/broccoli-stress-test \
  "${DIST}/stress-test/broccoli-stress-test-linux-amd64"
strip "${DIST}/stress-test/broccoli-stress-test-linux-amd64" || true

log "BUILD DONE. Dist tree:"
du -sh "${DIST}"/* | tee -a "${LOG_DIR}/build.log"
ls -la "${DIST}/images" "${DIST}/plugins" "${DIST}/stress-test" | tee -a "${LOG_DIR}/build.log"
