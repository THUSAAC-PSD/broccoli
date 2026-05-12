#!/usr/bin/env bash
# Collects every observability artifact + flamegraphs for the current profile run.
# Output: docs/profiling/run-2/artifacts/{prometheus,loki,jaeger,flamegraphs,docker}/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_CFG="${ROOT}/ops/ssh_config"
OUT="${ROOT}/docs/profiling/run-2/artifacts"
mkdir -p "${OUT}"/{prometheus,loki,jaeger,flamegraphs,docker,host-metrics}

log() { printf "\033[1;36m[%s]\033[0m %s\n" "$(date -u +%H:%M:%S)" "$*"; }

OBS_PRIV=10.104.0.15

# ---------- Prometheus TSDB snapshot ----------
log "Prometheus: triggering admin TSDB snapshot"
SNAP=$(ssh -F "${SSH_CFG}" broccoli-observability \
  "curl -sS -XPOST http://${OBS_PRIV}:9090/api/v1/admin/tsdb/snapshot" | jq -r '.data.name // empty')
if [[ -n "${SNAP}" ]]; then
  log "  snapshot: ${SNAP}"
  ssh -F "${SSH_CFG}" broccoli-observability \
    "tar -C /var/lib/docker/volumes/observability_prom_data/_data/snapshots/ -czf /root/prom-${SNAP}.tar.gz ${SNAP}/" || true
  scp -F "${SSH_CFG}" -q "broccoli-observability:/root/prom-${SNAP}.tar.gz" "${OUT}/prometheus/" 2>/dev/null || \
    log "  scp prom snapshot failed (may need root sudo for /var/lib/docker)"
fi

# Federate fallback: pull every metric for the run window.
log "Prometheus: federate-style query of all broccoli_* + node_* metrics"
ssh -F "${SSH_CFG}" broccoli-observability bash -s <<EOF > "${OUT}/prometheus/federate-snapshot.txt" 2>&1 || true
curl -sS -G "http://${OBS_PRIV}:9090/federate" \
  --data-urlencode 'match[]={__name__=~"broccoli_.*"}' \
  --data-urlencode 'match[]={__name__=~"node_.*"}' \
  --data-urlencode 'match[]={__name__=~"container_.*"}' \
  --data-urlencode 'match[]={__name__=~"redis_.*"}' \
  --data-urlencode 'match[]={__name__=~"pg_.*"}' \
  --data-urlencode 'match[]={job!=""}'
EOF

# ---------- Loki log dump ----------
log "Loki: dump last 6h of WARN/ERROR logs"
for stream in broccoli-server broccoli-worker postgres redis seaweed; do
  ssh -F "${SSH_CFG}" broccoli-observability \
    "curl -sS -G 'http://${OBS_PRIV}:3100/loki/api/v1/query_range' \
      --data-urlencode 'query={service=\"${stream}\"} |~ \"(?i)(error|warn|fail|timeout|panic|denied)\"' \
      --data-urlencode 'start='\$(date -d '6 hours ago' +%s%N) \
      --data-urlencode 'end='\$(date +%s%N) \
      --data-urlencode 'limit=5000'" \
    > "${OUT}/loki/${stream}-warn-error.json" 2>/dev/null || true
done

# ---------- Jaeger slow trace sampling ----------
log "Jaeger: pull 100 slowest broccoli-server / worker traces"
for svc in broccoli-server-api-1 broccoli-server-api-2 broccoli-worker-worker-1 broccoli-worker-worker-2; do
  ssh -F "${SSH_CFG}" broccoli-observability \
    "curl -sS 'http://${OBS_PRIV}:16686/api/traces?service=${svc}&limit=100&lookback=6h'" \
    > "${OUT}/jaeger/${svc}.json" 2>/dev/null || true
done

# ---------- docker-level summaries from every host ----------
log "Docker: container snapshots per host"
for h in broccoli-gateway broccoli-observability broccoli-loadgen \
         broccoli-postgres broccoli-redis broccoli-storage \
         broccoli-api-1 broccoli-api-2 broccoli-worker-1 broccoli-worker-2; do
  ssh -F "${SSH_CFG}" -o ConnectTimeout=10 "$h" 'docker ps -a --format "{{.Names}}\t{{.Status}}\t{{.Image}}"; echo "---"; docker stats --no-stream --format "{{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}" 2>/dev/null' \
    > "${OUT}/docker/${h}.txt" 2>/dev/null || true
done

# ---------- Host-level metrics from each ----------
log "Host metrics: vmstat/uptime/iostat snapshots"
for h in broccoli-gateway broccoli-observability broccoli-loadgen \
         broccoli-postgres broccoli-redis broccoli-storage \
         broccoli-api-1 broccoli-api-2 broccoli-worker-1 broccoli-worker-2; do
  ssh -F "${SSH_CFG}" -o ConnectTimeout=10 "$h" 'uptime; echo "--- vmstat ---"; vmstat 1 3; echo "--- iostat ---"; iostat -x 1 2 2>/dev/null | tail -30; echo "--- mem ---"; free -h; echo "--- net ---"; ss -s' \
    > "${OUT}/host-metrics/${h}.txt" 2>/dev/null || true
done

log "DONE. Artifacts in ${OUT}"
du -sh "${OUT}"/* 2>/dev/null
