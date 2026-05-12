#!/usr/bin/env bash
# Master deploy runner for the Broccoli profiling cluster.
# Run from the repo root: ./ops/deploy.sh
#
# Prereqs:
#   - ops/secrets/profiling.env exists
#   - ssh broccoli-gateway works (via ProxyJump-aware ops/ssh_config)
#   - dist/broccoli-platform-v0.0.0-local exists locally
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SECRETS="${ROOT}/ops/secrets/profiling.env"
BUNDLE="${ROOT}/dist/broccoli-platform-v0.0.0-local"
DEPLOY="${ROOT}/ops/deploy"
LOGS="${ROOT}/ops/logs"
SSH_CFG="${ROOT}/ops/ssh_config"

mkdir -p "${LOGS}"

source "${SECRETS}"

ssh() { command ssh -F "${SSH_CFG}" "$@"; }
scp() { command scp -F "${SSH_CFG}" "$@"; }

log() { printf "\033[1;36m[%s]\033[0m %s\n" "$(date -u +%H:%M:%S)" "$*"; }

# ============================================================
# Phase 0: Open sshd:443 on every host so SSH stays resilient.
# We already did this on gateway via DO console. Now do it on the
# rest via gateway-jumped SSH so we have a fallback if my SSH egress
# IP changes mid-run.
# ============================================================
phase_resilient_ssh() {
  log "Phase 0: opening sshd:443 on remaining hosts"
  local hosts=(broccoli-loadgen broccoli-observability broccoli-api-1 broccoli-api-2 \
               broccoli-postgres broccoli-redis broccoli-storage broccoli-worker-1 broccoli-worker-2)
  for h in "${hosts[@]}"; do
    (
      ssh -o ConnectTimeout=10 "$h" '
        if ! grep -q "^Port 443" /etc/ssh/sshd_config.d/99-tunnel.conf 2>/dev/null; then
          { echo "Port 22"; echo "Port 443"; } > /etc/ssh/sshd_config.d/99-tunnel.conf
          ufw allow 443/tcp >/dev/null
          systemctl restart ssh
        fi
        echo "sshd-443-ok"
      ' >"${LOGS}/${h}-sshd443.log" 2>&1
    ) &
  done
  wait
}

# ============================================================
# Phase 1: Distribute Broccoli image tar.gz files.
# Public images (postgres/redis/seaweedfs/caddy/prom/grafana/etc) are pulled
# from Docker Hub by docker compose on each host. Only Broccoli-specific
# images need to be shipped.
# ============================================================
phase_images() {
  log "Phase 1: distributing Broccoli image archives"
  # api-1, api-2 need server.tar.gz
  for h in broccoli-api-1 broccoli-api-2; do
    (
      log "  -> ${h}: server.tar.gz"
      scp -q "${BUNDLE}/images/server.tar.gz" "${h}:/root/server.tar.gz"
      ssh "$h" 'docker load -i /root/server.tar.gz'
    ) &
  done
  # worker-1, worker-2 need worker-icpc.tar.gz
  for h in broccoli-worker-1 broccoli-worker-2; do
    (
      log "  -> ${h}: worker-icpc.tar.gz"
      scp -q "${BUNDLE}/images/worker-icpc.tar.gz" "${h}:/root/worker-icpc.tar.gz"
      ssh "$h" 'docker load -i /root/worker-icpc.tar.gz'
    ) &
  done
  wait
}

# ============================================================
# Phase 2: Push compose dirs + secrets to each host.
# ============================================================
phase_configs() {
  log "Phase 2: pushing compose dirs and secrets"
  # avoid associative arrays (bash's [key]= syntax fights set -u with hyphens)
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
  done
  # Promtail on every host (including gateway/loadgen)
  for h in broccoli-gateway broccoli-loadgen broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    (
      ssh "$h" 'mkdir -p /opt/broccoli/promtail'
      rsync -az -e "ssh -F ${SSH_CFG}" "${DEPLOY}/promtail/" "$h:/opt/broccoli/promtail/"
    ) &
  done
  # Plugins on api-* and worker-*
  for h in broccoli-api-1 broccoli-api-2 broccoli-worker-1 broccoli-worker-2; do
    (
      log "  -> ${h}: plugins/"
      local target_dir
      case "$h" in
        broccoli-api-*)    target_dir=server ;;
        broccoli-worker-*) target_dir=worker ;;
      esac
      ssh "$h" "mkdir -p /opt/broccoli/${target_dir}/plugins"
      rsync -az -e "ssh -F ${SSH_CFG}" "${BUNDLE}/plugins/" "$h:/opt/broccoli/${target_dir}/plugins/"
    ) &
  done
  wait
}

# ============================================================
# Phase 3: Start infrastructure services (postgres, redis, storage).
# Must come before server/worker which depend on them.
# ============================================================
phase_infra() {
  log "Phase 3: starting infra (postgres, redis, storage)"
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
  done
  wait

  # Wait for healthy
  log "  waiting for infra healthchecks..."
  for h in broccoli-postgres broccoli-redis broccoli-storage; do
    local d
    case "$h" in
      broccoli-postgres) d=postgres ;;
      broccoli-redis)    d=redis ;;
      broccoli-storage)  d=storage ;;
    esac
    for try in $(seq 1 30); do
      if ssh "$h" "cd /opt/broccoli/${d} && docker compose ps --format json 2>/dev/null | grep -c 'healthy' " >/dev/null 2>&1; then
        echo "  ${h}: healthy"; break
      fi
      sleep 2
    done
  done
}

# ============================================================
# Phase 4: Observability stack.
# ============================================================
phase_observability() {
  log "Phase 4: starting observability"
  ssh broccoli-observability \
    "cd /opt/broccoli/observability && docker compose --env-file .env up -d" \
    >"${LOGS}/broccoli-observability-up.log" 2>&1
}

# ============================================================
# Phase 5: Broccoli servers (api-1, api-2).
# ============================================================
phase_servers() {
  log "Phase 5: starting Broccoli servers"
  for h in broccoli-api-1 broccoli-api-2; do
    local server_id="${h#broccoli-}"
    (
      log "  -> ${h} (BROCCOLI_SERVER_ID=${server_id})"
      ssh "$h" "cd /opt/broccoli/server && \
        BROCCOLI_SERVER_ID=${server_id} docker compose --env-file .env up -d" \
        >"${LOGS}/${h}-up.log" 2>&1
    ) &
  done
  wait
}

# ============================================================
# Phase 6: Broccoli workers (worker-1, worker-2).
# ============================================================
phase_workers() {
  log "Phase 6: starting Broccoli workers"
  for h in broccoli-worker-1 broccoli-worker-2; do
    local worker_id="${h#broccoli-}"
    (
      log "  -> ${h} (BROCCOLI_WORKER_ID=${worker_id})"
      ssh "$h" "cd /opt/broccoli/worker && \
        BROCCOLI_WORKER_ID=${worker_id} docker compose --env-file .env up -d" \
        >"${LOGS}/${h}-up.log" 2>&1
    ) &
  done
  wait
}

# ============================================================
# Phase 7: Caddy gateway.
# ============================================================
phase_gateway() {
  log "Phase 7: starting Caddy gateway"
  ssh broccoli-gateway \
    "cd /opt/broccoli/gateway && docker compose --env-file .env up -d" \
    >"${LOGS}/broccoli-gateway-up.log" 2>&1
}

# ============================================================
# Phase 8: Promtail everywhere.
# ============================================================
phase_promtail() {
  log "Phase 8: starting promtail on every host"
  for h in broccoli-gateway broccoli-loadgen broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    (
      ssh "$h" "cd /opt/broccoli/promtail && docker compose up -d" \
        >"${LOGS}/${h}-promtail.log" 2>&1
    ) &
  done
  wait
}

# ============================================================
# Phase 9: Push stress-test binary to loadgen.
# ============================================================
phase_loadgen() {
  log "Phase 9: configuring loadgen"
  ssh broccoli-loadgen 'mkdir -p /opt/broccoli/stress-test'
  scp -q "${BUNDLE}/stress-test/broccoli-stress-test" "broccoli-loadgen:/opt/broccoli/stress-test/"
  ssh broccoli-loadgen 'chmod +x /opt/broccoli/stress-test/broccoli-stress-test; /opt/broccoli/stress-test/broccoli-stress-test --version'
}

# ============================================================
# Phase 10: Health checks.
# ============================================================
phase_verify() {
  log "Phase 10: verifying cluster"
  echo "--- container status by host ---"
  for h in broccoli-gateway broccoli-loadgen broccoli-observability \
           broccoli-api-1 broccoli-api-2 broccoli-postgres broccoli-redis \
           broccoli-storage broccoli-worker-1 broccoli-worker-2; do
    printf "\n=== %s ===\n" "$h"
    ssh -o ConnectTimeout=5 "$h" 'docker ps --format "table {{.Names}}\t{{.Status}}"' 2>&1 || true
  done
  echo ""
  echo "--- HTTP gateway healthz ---"
  curl -s -m 5 http://178.128.218.138/healthz && echo " (gateway 200)" || echo " (gateway not ready)"
}

main() {
  case "${1:-all}" in
    all)
      phase_images
      phase_configs
      phase_infra
      phase_observability
      phase_servers
      phase_workers
      phase_gateway
      phase_promtail
      phase_loadgen
      phase_verify
      ;;
    ssh)      phase_resilient_ssh ;;
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
    *) echo "usage: $0 [all|ssh|images|configs|infra|obs|servers|workers|gateway|promtail|loadgen|verify]"; exit 1 ;;
  esac
}

main "$@"
