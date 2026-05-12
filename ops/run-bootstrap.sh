#!/usr/bin/env bash
# Runs ops/bootstrap.sh on every host in parallel.
# Per-host log goes to ops/logs/<host>-bootstrap.log
set -uo pipefail

ADMIN_IP="${ADMIN_IP:-59.66.0.0/16}"   # Tsinghua campus subnet (SSH egress)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="${SCRIPT_DIR}/logs"
mkdir -p "${LOG_DIR}"

# host:role:public_ip — run-2 (2026-05-12)
HOSTS=(
  "broccoli-build:build:209.97.162.248"
  "broccoli-gateway:gateway:206.189.84.60"
  "broccoli-observability:observability:165.22.102.240"
  "broccoli-loadgen:loadgen:168.144.101.175"
  "broccoli-postgres:postgres:159.89.196.18"
  "broccoli-redis:redis:165.232.174.195"
  "broccoli-storage:storage:68.183.226.122"
  "broccoli-api-1:api:209.97.170.94"
  "broccoli-api-2:api:139.59.119.194"
  "broccoli-worker-1:worker:168.144.104.193"
  "broccoli-worker-2:worker:159.65.137.233"
)

run_one() {
  local entry="$1"
  IFS=':' read -r name role ip <<<"${entry}"
  local log="${LOG_DIR}/${name}-bootstrap.log"
  {
    echo "[$(date -u +%FT%TZ)] >>> ${name} (${role}) ${ip}"
    scp -q -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="${SCRIPT_DIR}/known_hosts" \
      "${SCRIPT_DIR}/bootstrap.sh" "root@${ip}:/root/bootstrap.sh"
    ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="${SCRIPT_DIR}/known_hosts" \
      "root@${ip}" "bash /root/bootstrap.sh ${role} ${ADMIN_IP}"
    local rc=$?
    echo "[$(date -u +%FT%TZ)] <<< ${name} rc=${rc}"
    exit ${rc}
  } >"${log}" 2>&1
}

pids=()
for entry in "${HOSTS[@]}"; do
  run_one "${entry}" &
  pids+=($!)
done

fail=0
for i in "${!pids[@]}"; do
  if ! wait "${pids[$i]}"; then
    echo "FAIL: ${HOSTS[$i]}"
    fail=$((fail+1))
  else
    echo "ok:   ${HOSTS[$i]}"
  fi
done

echo
echo "Failures: ${fail}/${#HOSTS[@]}"
exit "${fail}"
