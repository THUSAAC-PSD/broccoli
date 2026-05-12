#!/usr/bin/env bash
# Continuous monitor during the stress run. Polls Prometheus + per-host
# vmstat snapshots every 60s and writes time-stamped lines to
# docs/profiling/run-2/snapshots/.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_CFG="${ROOT}/ops/ssh_config"
OUT="${ROOT}/docs/profiling/run-2/snapshots"
mkdir -p "${OUT}"

OBS_PRIV=10.104.0.15

PROM_QUERIES=(
  "sum(rate(broccoli_http_requests_total[1m]))|http_rps"
  "histogram_quantile(0.95, sum by (le) (rate(broccoli_http_request_duration_seconds_bucket[1m])))|http_p95_s"
  "sum(broccoli_mq_consume_inflight)|inflight"
  "sum(rate(broccoli_mq_consume_total[1m]))|mq_consume_rps"
  "avg(broccoli_database_pool_size)|db_pool"
  "avg(broccoli_database_pool_idle)|db_pool_idle"
  "sum(rate(broccoli_submission_judged_total[1m]))|judged_rps"
  "avg by (instance) (rate(node_cpu_seconds_total{mode!=\"idle\"}[1m]))|cpu_per_instance"
  "sum by (instance) (container_memory_working_set_bytes{name=~\"broccoli.*\"})|broccoli_mem_per_host"
)

while true; do
  ts=$(date -u +%FT%TZ)
  {
    echo "--- ${ts} ---"
    for q in "${PROM_QUERIES[@]}"; do
      query="${q%|*}"; label="${q##*|}"
      val=$(ssh -F "${SSH_CFG}" -o ConnectTimeout=8 broccoli-observability \
        "curl -sS -G 'http://${OBS_PRIV}:9090/api/v1/query' --data-urlencode 'query=${query}'" 2>/dev/null \
        | jq -c '.data.result' 2>/dev/null || echo "[]")
      echo "  ${label}: ${val:0:300}"
    done
  } >> "${OUT}/prometheus.log"

  # Brief per-host snapshot.
  for h in broccoli-api-1 broccoli-worker-1 broccoli-postgres broccoli-storage broccoli-redis; do
    ssh -F "${SSH_CFG}" -o ConnectTimeout=8 "$h" \
      "echo '[${ts} ${h}]'; uptime; free -h | grep Mem; vmstat 1 2 | tail -1" \
      >> "${OUT}/hosts.log" 2>/dev/null &
  done
  wait

  sleep 60
done
