#!/usr/bin/env bash
# Captures a perf-based CPU flame graph for broccoli-server (or worker) on the
# named host. Outputs an SVG to docs/profiling/run-2/artifacts/flamegraphs/.
#
# Usage: ./ops/perf-flamegraph.sh <host> <process-name> <window-seconds>
#   ./ops/perf-flamegraph.sh broccoli-api-1 broccoli-server 60
set -euo pipefail

HOST="${1:?host}"
PROC="${2:-broccoli-server}"
WIN="${3:-60}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_CFG="${ROOT}/ops/ssh_config"
OUT="${ROOT}/docs/profiling/run-2/artifacts/flamegraphs"
mkdir -p "${OUT}"

TS="$(date -u +%H%M%S)"
NAME="${HOST}-${PROC}-${TS}"

ssh -F "${SSH_CFG}" "${HOST}" bash -s <<EOF
set -euo pipefail
# Install perf + Brendan Gregg's FlameGraph scripts once.
if ! command -v perf >/dev/null 2>&1; then
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq linux-tools-common linux-tools-\$(uname -r) || \\
    apt-get install -y -qq linux-tools-generic >/dev/null
fi
if [[ ! -d /opt/FlameGraph ]]; then
  git clone --depth=1 https://github.com/brendangregg/FlameGraph /opt/FlameGraph >/dev/null
fi

# Find PID of the actual binary (skip tini wrapper inside container).
PID=\$(pgrep -x "${PROC}" 2>/dev/null | head -1)
if [[ -z "\${PID}" ]]; then
  # Fallback: docker inspect -> first child (skip tini)
  TINI=\$(docker inspect --format '{{.State.Pid}}' \$(docker ps --filter "name=${PROC}|server|worker" --format '{{.Names}}' | head -1) 2>/dev/null)
  PID=\$(pgrep -P "\${TINI}" 2>/dev/null | head -1)
fi
[[ -n "\${PID}" ]] || { echo "no PID for ${PROC}"; exit 1; }
echo "PID=\${PID} cmdline=\$(tr '\\000' ' ' </proc/\${PID}/cmdline)"

# Relax perf permissions (idempotent).
sysctl -w kernel.perf_event_paranoid=-1 >/dev/null 2>&1 || true
sysctl -w kernel.kptr_restrict=0 >/dev/null 2>&1 || true

cd /tmp
# --call-graph=dwarf needs full DWARF DIEs which our line-tables-only build lacks
# -> fall back to frame-pointer unwinding. Rust may omit FPs in release; in that
# case we still get useful flat top-N data per sample.
perf record -F 99 -p "\${PID}" -g --call-graph=fp -o /tmp/perf-${TS}.data -- sleep ${WIN}
perf script -i /tmp/perf-${TS}.data > /tmp/perf-${TS}.script
/opt/FlameGraph/stackcollapse-perf.pl /tmp/perf-${TS}.script > /tmp/perf-${TS}.folded
/opt/FlameGraph/flamegraph.pl --title "${PROC} on ${HOST} @ ${TS} (${WIN}s sample)" /tmp/perf-${TS}.folded > /tmp/flame-${NAME}.svg
ls -la /tmp/flame-${NAME}.svg /tmp/perf-${TS}.folded
EOF

scp -F "${SSH_CFG}" -q "${HOST}:/tmp/flame-${NAME}.svg" "${OUT}/" || echo "scp failed"
scp -F "${SSH_CFG}" -q "${HOST}:/tmp/perf-${TS}.folded" "${OUT}/perf-${NAME}.folded" || true

echo "wrote ${OUT}/flame-${NAME}.svg"
