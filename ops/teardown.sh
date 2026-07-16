#!/usr/bin/env bash
# Destroys all 10 profiling droplets. Idempotent - re-runnable even after partial deletes.
# Requires DO MCP tool access; this script uses doctl if available, otherwise prints
# the droplet IDs for manual deletion / MCP invocation by the caller.
set -uo pipefail

IDS=(
  570385691   # broccoli-gateway
  570385705   # broccoli-loadgen
  570385707   # broccoli-observability
  570385713   # broccoli-api-1
  570385718   # broccoli-api-2
  570385721   # broccoli-redis
  570385725   # broccoli-storage
  570385737   # broccoli-worker-1
  570385747   # broccoli-worker-2
  570385802   # broccoli-postgres
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
  cat <<EOF
doctl not available. Use the DO MCP server tools instead. Droplet IDs to delete:
${IDS[@]}

In Claude:
  for each id, call mcp__digitalocean__droplet-delete with ID=<id>
EOF
fi
