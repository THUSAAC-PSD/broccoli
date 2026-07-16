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

# --- 2. Support images (postgres, redis, seaweed, caddy) - pull + save ---
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

# Read a single `key = "value"` string from one section of a plugin.toml.
manifest_value() { # <file> <section> <key>
  awk -v sec="[$2]" -v key="$3" '
    /^[[:space:]]*\[/ { hdr = $0; gsub(/[[:space:]]/, "", hdr); insec = (hdr == sec); next }
    insec && $0 ~ "^[[:space:]]*"key"[[:space:]]*=" {
      sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit
    }
  ' "$1"
}

# Read every `key = "value"` string in one section of a plugin.toml (one per line).
manifest_section_values() { # <file> <section>
  awk -v sec="[$2]" '
    /^[[:space:]]*\[/ { hdr = $0; gsub(/[[:space:]]/, "", hdr); insec = (hdr == sec); next }
    insec && /^[^=#]+=[[:space:]]*"/ { sub(/^[^"]*"/, ""); sub(/".*$/, ""); print }
  ' "$1"
}

# Read the items of a single-line string array `key = ["a", "b"]` (one per line).
manifest_array_values() { # <file> <section> <key>
  awk -v sec="[$2]" -v key="$3" '
    /^[[:space:]]*\[/ { hdr = $0; gsub(/[[:space:]]/, "", hdr); insec = (hdr == sec); next }
    insec && $0 ~ "^[[:space:]]*"key"[[:space:]]*=" {
      sub(/^[^[]*\[/, ""); sub(/\].*$/, "")
      n = split($0, items, ",")
      for (i = 1; i <= n; i++) {
        gsub(/"/, "", items[i]); gsub(/^[[:space:]]+|[[:space:]]+$/, "", items[i])
        if (items[i] != "") print items[i]
      }
      exit
    }
  ' "$1"
}

# The host dereferences every manifest path from the deployed plugin dir at
# load/activation time (server entry wasm, [translations] files, [web] assets),
# so the dist bundle must contain all of them or the plugin fails to activate.
MISSING_PLUGIN_ASSETS=0
require_asset() { # <plugin name> <path relative to the plugin dir>
  if [[ ! -e "${DIST}/plugins/$1/$2" ]]; then
    log "  ERROR: plugin '$1' manifest references '$2' but it is missing from the dist bundle"
    MISSING_PLUGIN_ASSETS=1
  fi
}

for d in plugins/*/; do
  name="$(basename "$d")"
  if [[ ! -f "${d}Cargo.toml" || ! -f "${d}plugin.toml" ]]; then continue; fi
  log "  plugin: ${name}"
  ( cd "$d" && cargo build --target wasm32-wasip1 --release 2>&1 | tail -3 ) \
    >>"${LOG_DIR}/plugins.log" 2>&1
  out="${DIST}/plugins/${name}"
  mkdir -p "${out}"
  cp "${d}plugin.toml" "${out}/"
  wasm="$(find "${d}target/wasm32-wasip1/release/" -maxdepth 1 -name '*.wasm' ! -name 'deps' | head -1)"
  if [[ -n "$wasm" ]]; then cp "$wasm" "${out}/"; fi
  # Install the built wasm under every declared entry name. Also install into the
  # source tree so the manifest entry path resolves there (some plugins reference
  # name.wasm relative to the plugin dir).
  for section in server worker; do
    entry="$(manifest_value "${d}plugin.toml" "$section" entry)"
    if [[ -n "$entry" ]]; then
      if [[ -n "$wasm" ]]; then
        mkdir -p "$(dirname "${out}/${entry}")"
        cp "$wasm" "${d}${entry}" 2>/dev/null || true
        cp "$wasm" "${out}/${entry}"
      fi
      require_asset "$name" "$entry"
    fi
  done
  # [translations]: activate_plugin reads each file from the plugin root; a
  # missing translation file makes activation fail on the deployed host.
  while IFS= read -r tpath; do
    if [[ -z "$tpath" ]]; then continue; fi
    if [[ -f "${d}${tpath}" ]]; then
      mkdir -p "$(dirname "${out}/${tpath}")"
      cp "${d}${tpath}" "${out}/${tpath}"
    fi
    require_asset "$name" "$tpath"
  done < <(manifest_section_values "${d}plugin.toml" translations)
  # [web]: the server serves entry/css out of <plugin dir>/<web.root>.
  webroot="$(manifest_value "${d}plugin.toml" web root)"
  if [[ -n "$webroot" && ( "$webroot" == /* || "$webroot" == *..* ) ]]; then
    log "  ERROR: plugin '${name}' declares unsafe [web] root '${webroot}'"
    MISSING_PLUGIN_ASSETS=1
  elif [[ -n "$webroot" ]]; then
    if [[ -d "${d}${webroot}" ]]; then
      mkdir -p "$(dirname "${out}/${webroot}")"
      rm -rf "${out:?}/${webroot}"
      cp -a "${d}${webroot}" "${out}/${webroot}"
    fi
    require_asset "$name" "$webroot"
    webentry="$(manifest_value "${d}plugin.toml" web entry)"
    if [[ -n "$webentry" ]]; then require_asset "$name" "${webroot}/${webentry}"; fi
    while IFS= read -r css; do
      if [[ -n "$css" ]]; then require_asset "$name" "${webroot}/${css}"; fi
    done < <(manifest_array_values "${d}plugin.toml" web css)
  fi
done

if (( MISSING_PLUGIN_ASSETS )); then
  log "FATAL: plugin dist bundle is incomplete; deployed plugins would fail to activate. Aborting."
  exit 1
fi

# --- 4. stress-test linux/amd64 musl static binary ---
log "BUILD: stress-test (musl)..."
cargo build -p stress-test --release --target x86_64-unknown-linux-musl 2>&1 | tee "${LOG_DIR}/stress-test.log" | tail -10
cp target/x86_64-unknown-linux-musl/release/broccoli-stress-test \
  "${DIST}/stress-test/broccoli-stress-test-linux-amd64"
strip "${DIST}/stress-test/broccoli-stress-test-linux-amd64" || true

log "BUILD DONE. Dist tree:"
du -sh "${DIST}"/* | tee -a "${LOG_DIR}/build.log"
ls -la "${DIST}/images" "${DIST}/plugins" "${DIST}/stress-test" | tee -a "${LOG_DIR}/build.log"
