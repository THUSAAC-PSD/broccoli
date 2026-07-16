#!/usr/bin/env bash
# Destroys all 11 profiling-run-2 droplets. Idempotent - re-runnable.
set -uo pipefail

IDS=(
  570404180   # broccoli-build
  570404183   # broccoli-gateway
  570404188   # broccoli-observability
  570404197   # broccoli-loadgen
  570404201   # broccoli-postgres
  570404209   # broccoli-redis
  570404221   # broccoli-storage
  570404223   # broccoli-api-1
  570404228   # broccoli-api-2
  570404231   # broccoli-worker-1
  570404236   # broccoli-worker-2
)

echo "About to destroy ${#IDS[@]} droplets:"
printf "  - %s\n" "${IDS[@]}"
echo

if command -v doctl >/dev/null 2>&1; then
  for id in "${IDS[@]}"; do
    echo "destroying ${id}..."
    doctl compute droplet delete --force "${id}" || true
  done
  echo "done"
else
  echo "doctl not present. Use DO MCP droplet-delete for each id above."
fi
