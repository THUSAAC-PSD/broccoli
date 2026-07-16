#!/usr/bin/env bash
# Triggers the mixed-mode stress test on broccoli-loadgen.
# Run from local mac. Streams JSON output to docs/profiling/run-2/stress-output.jsonl
# and a human summary to stress-summary.log.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_CFG="${ROOT}/ops/ssh_config"

OUT_DIR="${ROOT}/docs/profiling/run-2"
mkdir -p "${OUT_DIR}"

# Target the gateway's private VPC address from the loadgen (same VPC).
# The gateway only serves plain HTTP (:80, no TLS), so traffic must never
# cross the public internet. Auth uses ADMIN_TOKEN from run-state.env
# (provisioned by seed-signpost.sh) so the admin password never leaves the
# operator machine or appears in the remote command line.
GATEWAY_PRIV="10.104.0.14"

# Stress-test target sizing (signpost / 5000 contestants / mixed):
#   - 5000 contestant accounts (bulk-registered up front)
#   - rate=150 actions/s ~ 5000 x 1 action / 33s - typical contest steady state
#   - concurrency 200: in-flight cap, ~1.3s avg per action
#   - duration 9000s (2.5 hr) steady + 1800s (30 min) burst at 3x
#   - per-job timeout 60s; p95 budget 15s
DURATION="${DURATION:-9000}"
RATE="${RATE:-150}"
CONCURRENCY="${CONCURRENCY:-200}"
CONTESTANTS="${CONTESTANTS:-5000}"
BURST_DURATION="${BURST_DURATION:-1800}"
BURST_MULT="${BURST_MULT:-3}"
PER_JOB_TIMEOUT="${PER_JOB_TIMEOUT:-60}"
P95_BUDGET="${P95_BUDGET:-15000}"
RUN_ID="${RUN_ID:-signpost-2026-05-12}"

LOADGEN_CMD=$(cat <<EOF
set -euo pipefail
cd /opt/broccoli/stress-test
source ./run-state.env
: "\${ADMIN_TOKEN:?run-state.env is missing ADMIN_TOKEN; re-run seed-signpost.sh}"

# total = rate x duration (sanity bound)
TOTAL=\$((${RATE} * ${DURATION}))
exec ./broccoli-stress-test \\
  --url "http://${GATEWAY_PRIV}" \\
  --admin-token "\${ADMIN_TOKEN}" \\
  --profile mixed \\
  --duration ${DURATION} \\
  --rate ${RATE} \\
  --concurrency ${CONCURRENCY} \\
  --contestants ${CONTESTANTS} \\
  --final-burst-duration ${BURST_DURATION} \\
  --final-burst-multiplier ${BURST_MULT} \\
  --per-job-timeout ${PER_JOB_TIMEOUT} \\
  --p95-budget-ms ${P95_BUDGET} \\
  --total \${TOTAL} \\
  --contest-id \${CONTEST_ID} \\
  --problem-id \${PROBLEM_ID} \\
  --contest-type \${CONTEST_TYPE} \\
  --problem-type \${PROBLEM_TYPE} \\
  --run-id "${RUN_ID}" \\
  --keep-fixtures \\
  --skip-correctness \\
  --json
EOF
)

echo "[$(date -u +%H:%M:%S)] starting stress-test on loadgen..."
echo "  RUN_ID=${RUN_ID} DURATION=${DURATION}s RATE=${RATE}/s CONTESTANTS=${CONTESTANTS}"
echo "  BURST: +${BURST_DURATION}s at ${BURST_MULT}x"
echo "  output: ${OUT_DIR}/stress-output.jsonl"

# Run on loadgen, stream stdout to a local file. Use a large pipe buffer.
ssh -F "${SSH_CFG}" -o ServerAliveInterval=30 broccoli-loadgen "bash -c '${LOADGEN_CMD}'" \
  | tee "${OUT_DIR}/stress-output.jsonl"
