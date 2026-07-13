#!/usr/bin/env bash
# Run-2 deploy: ships images from broccoli-build (VPC) → deploy hosts,
# pushes compose dirs + secrets from local mac, and brings up services in order.
# Usage: ./ops/deploy-run2.sh [all|images|configs|infra|obs|servers|workers|gateway|promtail|loadgen|verify]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT}"

SECRETS="${ROOT}/ops/secrets/profiling.env"
DEPLOY="${ROOT}/ops/deploy"
LOGS="${ROOT}/ops/logs/run2"
SSH_CFG="${ROOT}/ops/ssh_config"

mkdir -p "${LOGS}"

source "${SECRETS}"

# shellcheck disable=SC2032
ssh() { command ssh -F "${SSH_CFG}" -o ServerAliveInterval=30 "$@"; }
# shellcheck disable=SC2032
scp() { command scp -F "${SSH_CFG}" -o ServerAliveInterval=30 "$@"; }
log() { printf "\033[1;36m[%s]\033[0m %s\n" "$(date -u +%H:%M:%S)" "$*"; }

# Wait on each background PID individually and fail if any child failed.
# A bare `wait` always returns 0 and `set -e` does not apply to background
# jobs, so parallel phases must funnel their PIDs through this helper.
wait_all() {
  local rc=0 pid
  for pid in "$@"; do
    if ! wait "${pid}"; then
      rc=1
    fi
  done
  if [ "${rc}" -ne 0 ]; then
    log "ERROR: a parallel job failed; aborting"
    return 1
  fi
}

GATEWAY_PUB="206.189.84.60"   # new run-2 gateway public IP

# Private IP last-octet by host name — never actually consulted at run time
# (deploy ships images via hard-coded VPC IPs below). Kept as documentation.
# Avoiding `declare -A` here because hyphenated keys + `set -u` cause unbound-var
# crashes in bash 5.

# ============================================================
# Phase 1: Distribute Broccoli image tar.gz files from build droplet via VPC.
# Build droplet has /root/broccoli-dist/images/*.tar.gz and ed25519 SSH key
# pre-authorized on every deploy host.
# ============================================================
phase_images() {
  local tag="${TAG:-profile2}"
  log "Phase 1: distributing images from broccoli-build via VPC (TAG=${tag})"

  ssh broccoli-build "TAG='${tag}' bash -s" <<'REMOTE'
set -euo pipefail
DIST=/root/broccoli-dist
TAG="${TAG:-profile2}"
KEY=~/.ssh/id_ed25519

push_server() {
  local remote="$1"
  echo "  -> ${remote}: server.tar.gz"
  scp -q -i ${KEY} -o StrictHostKeyChecking=no "${DIST}/images/server.tar.gz" "root@${remote}:/root/server.tar.gz"
  ssh -i ${KEY} -o StrictHostKeyChecking=no "root@${remote}" \
    "docker load -i /root/server.tar.gz >/dev/null && docker tag broccoli-server:${TAG} broccoli-server:v0.0.0-local"
}
push_worker() {
  local remote="$1"
  echo "  -> ${remote}: worker-icpc.tar.gz"
  scp -q -i ${KEY} -o StrictHostKeyChecking=no "${DIST}/images/worker-icpc.tar.gz" "root@${remote}:/root/worker-icpc.tar.gz"
  ssh -i ${KEY} -o StrictHostKeyChecking=no "root@${remote}" \
    "docker load -i /root/worker-icpc.tar.gz >/dev/null && docker tag broccoli-worker:${TAG}-icpc broccoli-worker:v0.0.0-local-icpc"
}
push_simple() {
  local img="$1" remote="$2"
  echo "  -> ${remote}: ${img}"
  scp -q -i ${KEY} -o StrictHostKeyChecking=no "${DIST}/images/${img}" "root@${remote}:/root/${img}"
  ssh -i ${KEY} -o StrictHostKeyChecking=no "root@${remote}" "docker load -i /root/${img} >/dev/null"
}

pids=()
( push_server 10.104.0.20 ) & pids+=($!)
( push_server 10.104.0.21 ) & pids+=($!)
( push_worker 10.104.0.22 ) & pids+=($!)
( push_worker 10.104.0.23 ) & pids+=($!)

# Support images (one per relevant host).
( push_simple postgres.tar.gz  10.104.0.17 ) & pids+=($!)
( push_simple redis.tar.gz     10.104.0.18 ) & pids+=($!)
( push_simple seaweedfs.tar.gz 10.104.0.19 ) & pids+=($!)
( push_simple caddy.tar.gz     10.104.0.14 ) & pids+=($!)

# A bare `wait` returns 0 even if children failed; check each one.
rc=0
for pid in "${pids[@]}"; do
  wait "${pid}" || rc=1
done
if [ "${rc}" -ne 0 ]; then
  echo "ERROR: image distribution failed on at least one host" >&2
  exit 1
fi
echo "image distribution done"
REMOTE
}

# ============================================================
# Phase 2: Push compose dirs + secrets to each host.
# ============================================================
phase_configs() {
  log "Phase 2: pushing compose dirs and secrets"
  local pids=()
  local pairs=(
    "broccoli-postgres:postgres"
    "broccoli-redis:redis"
    "broccoli-storage:storage"
    "broccoli-observability:observability"
    "broccoli-api-1:server"
    "broccoli-api-2:server"
    "broccoli-worker-1:worker"
    "broccoli-worker-2:worker"
    "broccoli-gateway:gateway"
  )
  for pair in "${pairs[@]}"; do
    local h="${pair%%:*}"
    local d="${pair##*:}"
    (
      log "  -> ${h}: ${d}/"
      ssh "$h" 'mkdir -p /opt/broccoli'
      rsync -az -e "ssh -F ${SSH_CFG}" "${DEPLOY}/${d}/" "$h:/opt/broccoli/${d}/"
      scp -q "${SECRETS}" "$h:/opt/broccoli/${d}/.env"
    ) &
    pids+=($!)
  done
  # Promtail on every host
  for h in broccoli-gateway broccoli-loadgen broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    (
      ssh "$h" 'mkdir -p /opt/broccoli/promtail'
      rsync -az -e "ssh -F ${SSH_CFG}" "${DEPLOY}/promtail/" "$h:/opt/broccoli/promtail/"
    ) &
    pids+=($!)
  done
  wait_all "${pids[@]}"

  # Plugins on api-* and worker-*: pull from build droplet via VPC.
  log "  pulling plugins from broccoli-build to api/worker hosts via VPC..."
  ssh broccoli-build bash <<'REMOTE'
set -euo pipefail
DIST=/root/broccoli-dist
KEY=~/.ssh/id_ed25519
for h in 10.104.0.20 10.104.0.21 10.104.0.22 10.104.0.23; do
  case "${h}" in
    10.104.0.20|10.104.0.21) target=/opt/broccoli/server/plugins ;;
    10.104.0.22|10.104.0.23) target=/opt/broccoli/worker/plugins ;;
  esac
  ssh -i ${KEY} -o StrictHostKeyChecking=no "root@${h}" "mkdir -p ${target}"
  rsync -az -e "ssh -i ${KEY} -o StrictHostKeyChecking=no" "${DIST}/plugins/" "root@${h}:${target}/"
done
echo "plugins distributed"
REMOTE
}

# ============================================================
# Phase 3: infra (postgres, redis, storage)
# ============================================================
phase_infra() {
  log "Phase 3: starting infra"
  local pids=()
  for h in broccoli-postgres broccoli-redis broccoli-storage; do
    local d
    case "$h" in
      broccoli-postgres) d=postgres ;;
      broccoli-redis)    d=redis ;;
      broccoli-storage)  d=storage ;;
    esac
    (
      log "  -> ${h}"
      ssh "$h" "cd /opt/broccoli/${d} && docker compose --env-file .env up -d" \
        >"${LOGS}/${h}-up.log" 2>&1
    ) &
    pids+=($!)
  done
  wait_all "${pids[@]}"
  sleep 8
  log "  infra status:"
  for h in broccoli-postgres broccoli-redis broccoli-storage; do
    ssh -o ConnectTimeout=10 "$h" 'docker ps --format "  {{.Names}} {{.Status}}"' || true
  done
}

phase_observability() {
  log "Phase 4: starting observability"
  ssh broccoli-observability "cd /opt/broccoli/observability && docker compose --env-file .env up -d" \
    >"${LOGS}/broccoli-observability-up.log" 2>&1
}

phase_servers() {
  log "Phase 5: starting Broccoli servers"
  local pids=()
  for h in broccoli-api-1 broccoli-api-2; do
    local server_id="${h#broccoli-}"
    (
      log "  -> ${h} (BROCCOLI_SERVER_ID=${server_id})"
      ssh "$h" "cd /opt/broccoli/server && BROCCOLI_SERVER_ID=${server_id} docker compose --env-file .env up -d" \
        >"${LOGS}/${h}-up.log" 2>&1
    ) &
    pids+=($!)
  done
  wait_all "${pids[@]}"
}

phase_workers() {
  log "Phase 6: starting Broccoli workers"
  local pids=()
  for h in broccoli-worker-1 broccoli-worker-2; do
    local worker_id="${h#broccoli-}"
    (
      log "  -> ${h} (BROCCOLI_WORKER_ID=${worker_id})"
      ssh "$h" "cd /opt/broccoli/worker && BROCCOLI_WORKER_ID=${worker_id} docker compose --env-file .env up -d" \
        >"${LOGS}/${h}-up.log" 2>&1
    ) &
    pids+=($!)
  done
  wait_all "${pids[@]}"
}

phase_gateway() {
  log "Phase 7: starting Caddy gateway"
  ssh broccoli-gateway "cd /opt/broccoli/gateway && docker compose --env-file .env up -d" \
    >"${LOGS}/broccoli-gateway-up.log" 2>&1
}

phase_promtail() {
  log "Phase 8: starting promtail on every host"
  local pids=()
  for h in broccoli-gateway broccoli-loadgen broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    (
      ssh "$h" "cd /opt/broccoli/promtail && docker compose up -d" \
        >"${LOGS}/${h}-promtail.log" 2>&1
    ) &
    pids+=($!)
  done
  wait_all "${pids[@]}"
}

phase_loadgen() {
  log "Phase 9: configuring loadgen (pull stress-test from build)"
  ssh broccoli-build bash <<'REMOTE'
set -euo pipefail
ssh -i ~/.ssh/id_ed25519 -o StrictHostKeyChecking=no root@10.104.0.16 'mkdir -p /opt/broccoli/stress-test'
scp -q -i ~/.ssh/id_ed25519 -o StrictHostKeyChecking=no \
  /root/broccoli-dist/stress-test/broccoli-stress-test-linux-amd64 \
  root@10.104.0.16:/opt/broccoli/stress-test/broccoli-stress-test
ssh -i ~/.ssh/id_ed25519 -o StrictHostKeyChecking=no root@10.104.0.16 \
  'chmod +x /opt/broccoli/stress-test/broccoli-stress-test && /opt/broccoli/stress-test/broccoli-stress-test --version'
REMOTE
}

phase_verify() {
  log "Phase 10: verifying cluster"
  for h in broccoli-gateway broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    printf "\n=== %s ===\n" "$h"
    ssh -o ConnectTimeout=10 "$h" 'docker ps --format "table {{.Names}}\t{{.Status}}"' 2>&1 || true
  done
  echo ""
  echo "--- HTTP gateway healthz ---"
  curl -s -m 10 "http://${GATEWAY_PUB}/healthz" && echo " (gateway 200)" || echo " (gateway not ready)"
}

main() {
  case "${1:-all}" in
    all)
      phase_images; phase_configs; phase_infra; phase_observability
      phase_servers; phase_workers; phase_gateway; phase_promtail
      phase_loadgen; phase_verify
      ;;
    images)   phase_images ;;
    configs)  phase_configs ;;
    infra)    phase_infra ;;
    obs)      phase_observability ;;
    servers)  phase_servers ;;
    workers)  phase_workers ;;
    gateway)  phase_gateway ;;
    promtail) phase_promtail ;;
    loadgen)  phase_loadgen ;;
    verify)   phase_verify ;;
    *) echo "usage: $0 [all|images|configs|infra|obs|servers|workers|gateway|promtail|loadgen|verify]"; exit 1 ;;
  esac
}

main "$@"
