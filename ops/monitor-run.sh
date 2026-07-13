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

# Metric names MUST match the exporter's actual /metrics output
# (opentelemetry-prometheus, no namespace: instrument dots become
# underscores, counters gain `_total`, seconds-unit histograms gain
# `_seconds`). Source of truth: packages/common/src/metrics.rs, or
# `curl http://<api-host>:<metrics-port>/metrics`.
# Entry format: "<promql>|<label>" -- the label is everything after
# the LAST `|`, so `|` inside a PromQL regex matcher is fine.
PROM_QUERIES=(
  "sum(rate(http_server_request_total[1m]))|http_rps"
  "histogram_quantile(0.95, sum by (le) (rate(http_server_request_duration_seconds_bucket[1m])))|http_p95_s"
  "sum(broccoli_task_in_flight)|inflight"
  "sum(rate(broccoli_mq_consume_messages_total[1m]))|mq_consume_rps"
  "sum(broccoli_submission_judge_queue_depth)|judge_queue_depth"
  "sum(broccoli_submission_in_flight)|submissions_in_flight"
  "sum(rate(broccoli_submission_state_transition_duration_seconds_count{to=~\"Judged|CompilationError|SystemError\"}[1m]))|judged_rps"
  "avg by (instance) (rate(node_cpu_seconds_total{mode!=\"idle\"}[1m]))|cpu_per_instance"
  "sum by (instance) (container_memory_working_set_bytes{name=~\"broccoli.*\"})|broccoli_mem_per_host"
)

first_poll=1

while true; do
  ts=$(date -u +%FT%TZ)
  {
    echo "--- ${ts} ---"
    for q in "${PROM_QUERIES[@]}"; do
      query="${q%|*}"; label="${q##*|}"
      resp=$(ssh -F "${SSH_CFG}" -o ConnectTimeout=8 broccoli-observability \
        "curl -sS -G 'http://${OBS_PRIV}:9090/api/v1/query' --data-urlencode 'query=${query}'" 2>/dev/null)
      if [ -z "${resp}" ]; then
        # ssh/curl failure is NOT the same as an empty result set.
        val="FETCH_ERROR"
      else
        val=$(jq -cr 'if .status == "success" then .data.result else "QUERY_ERROR: \(.error // .errorType // "unknown")" end' \
          <<<"${resp}" 2>/dev/null) || val="BAD_RESPONSE"
      fi
      echo "  ${label}: ${val:0:300}"
      # Self-check: an empty vector on the very first poll almost always
      # means a mis-spelled metric name or a down scrape target, not zero
      # traffic. Warn loudly instead of silently logging [] all run.
      if [ "${first_poll}" -eq 1 ]; then
        case "${val}" in
          "[]"|FETCH_ERROR|BAD_RESPONSE|"QUERY_ERROR"*)
            warn="  WARN: '${label}' returned no data on first poll (query: ${query}); check the metric name against the server's /metrics output and that Prometheus is scraping"
            echo "${warn}"
            echo "${warn}" >&2
            ;;
        esac
      fi
    done
  } >> "${OUT}/prometheus.log"
  first_poll=0

  # Brief per-host snapshot.
  for h in broccoli-api-1 broccoli-worker-1 broccoli-postgres broccoli-storage broccoli-redis; do
    ssh -F "${SSH_CFG}" -o ConnectTimeout=8 "$h" \
      "echo '[${ts} ${h}]'; uptime; free -h | grep Mem; vmstat 1 2 | tail -1" \
      >> "${OUT}/hosts.log" 2>/dev/null &
  done
  wait

  sleep 60
done
