#!/usr/bin/env bash
# Single entry point for a Broccoli profiling run.
# Once the cluster is provisioned, every step is automated:
#   ./ops/profile-runbook.sh provision  # spin up 10 deploy + 1 build droplet
#   ./ops/profile-runbook.sh bootstrap  # apt/docker/ufw on all hosts
#   ./ops/profile-runbook.sh build      # rsync source + docker build images (blocks until done)
#   ./ops/profile-runbook.sh build-wait # resume waiting on an in-flight build (e.g. after ^C)
#   ./ops/profile-runbook.sh deploy     # distribute images & bring up services (refuses mid-build)
#   ./ops/profile-runbook.sh seed       # create Signpost contest + problem
#   ./ops/profile-runbook.sh stress     # run mixed-mode stress test
#   ./ops/profile-runbook.sh monitor    # live monitor (60s polls; run in background)
#   ./ops/profile-runbook.sh flames     # capture 3 perf flame graphs
#   ./ops/profile-runbook.sh collect    # pull all observability artifacts
#   ./ops/profile-runbook.sh teardown   # destroy droplets
#   ./ops/profile-runbook.sh all        # run everything end to end (multi-hour)
#
# Iterate on optimizations by:
#   1. Edit source.
#   2. ./ops/profile-runbook.sh build && ./ops/profile-runbook.sh deploy
#   3. ./ops/profile-runbook.sh seed (or skip if reusing contest)
#   4. ./ops/profile-runbook.sh stress
#   5. ./ops/profile-runbook.sh collect
#
# Cluster state lives in ops/profiling-inventory-run2.yaml; update it after
# provision (or write a fresh inventory for a new run).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OPS="${ROOT}/ops"

phase_provision() {
  echo "Provisioning is an MCP-driven step (see docs/profiling/journal.md run-2)."
  echo "Once droplets are created, write a fresh ops/profiling-inventory-runN.yaml"
  echo "and update ops/ssh_config + ops/run-bootstrap.sh with the new IPs."
}

phase_bootstrap() { "${OPS}/run-bootstrap.sh"; }

# Sentinel written on the build droplet once remote-build.sh fully finishes:
# absent while a build is in flight, contains the build's exit code afterwards.
# docker save leaves the PREVIOUS run's tars in ${DIST}/images, so deploying
# without checking this sentinel silently ships (and profiles) stale images.
BUILD_SENTINEL="/root/broccoli-dist/build.exit"

phase_build() {
  rsync -avzP --exclude='target/' --exclude='node_modules/' --exclude='.git/' \
    --exclude='dist/' --exclude='qa-evidence/' --exclude='ops/logs/' \
    --exclude='ops/secrets/' --exclude='.DS_Store' \
    -e "ssh -F ${OPS}/ssh_config" "${ROOT}/" broccoli-build:/root/broccoli/
  scp -F "${OPS}/ssh_config" -q "${OPS}/remote-build.sh" broccoli-build:/root/remote-build.sh
  # Detached (survives ssh drops) but not fire-and-forget: the wrapper records
  # the exit code in the sentinel as its last act, and we block on it below.
  ssh -F "${OPS}/ssh_config" broccoli-build \
    "set -e; mkdir -p /root/broccoli-dist; rm -f ${BUILD_SENTINEL}; nohup bash -c 'bash /root/remote-build.sh > /root/build.log 2>&1; echo \$? > ${BUILD_SENTINEL}' >/dev/null 2>&1 & echo \"build launched (PID \$!)\""
  echo "Tail with: ssh -F ${OPS}/ssh_config broccoli-build 'tail -f /root/build.log'"
  phase_build_wait
}

phase_build_wait() {
  local poll=30
  local timeout="${BUILD_WAIT_TIMEOUT:-14400}"
  local deadline=$(( $(date +%s) + timeout ))
  local exit_code=""
  echo "Waiting for remote build to finish (polling every ${poll}s, timeout ${timeout}s)..."
  while :; do
    exit_code="$(ssh -F "${OPS}/ssh_config" -o ConnectTimeout=15 broccoli-build \
      "cat ${BUILD_SENTINEL} 2>/dev/null" 2>/dev/null || true)"
    [[ -n "${exit_code}" ]] && break
    if (( $(date +%s) >= deadline )); then
      echo "ERROR: remote build did not finish within ${timeout}s." >&2
      echo "Keep waiting with: ./ops/profile-runbook.sh build-wait (raise BUILD_WAIT_TIMEOUT if needed)" >&2
      return 1
    fi
    sleep "${poll}"
  done
  if [[ "${exit_code}" != "0" ]]; then
    echo "ERROR: remote build failed (exit ${exit_code})." >&2
    echo "Inspect: ssh -F ${OPS}/ssh_config broccoli-build 'tail -100 /root/build.log'" >&2
    return 1
  fi
  echo "Remote build finished OK."
}

# Refuse to deploy unless the last launched build completed successfully:
# sentinel absent = build in flight (or never run), non-zero = build failed.
# Either way the tars on disk do not match the source that was just synced.
require_build_ok() {
  if [[ "${FORCE_DEPLOY:-0}" == "1" ]]; then
    echo "WARNING: FORCE_DEPLOY=1 set; skipping build-completion check." >&2
    return 0
  fi
  local exit_code
  exit_code="$(ssh -F "${OPS}/ssh_config" broccoli-build \
    "cat ${BUILD_SENTINEL} 2>/dev/null" || true)"
  if [[ "${exit_code}" != "0" ]]; then
    if [[ -z "${exit_code}" ]]; then
      echo "ERROR: no completed build found (${BUILD_SENTINEL} missing on broccoli-build)." >&2
      echo "A build may still be running: ./ops/profile-runbook.sh build-wait" >&2
    else
      echo "ERROR: last build failed (exit ${exit_code}); refusing to deploy its stale images." >&2
    fi
    echo "Run ./ops/profile-runbook.sh build first, or set FORCE_DEPLOY=1 to override." >&2
    return 1
  fi
}

phase_deploy() { require_build_ok; "${OPS}/deploy-run2.sh" all; }
phase_seed()   { "${OPS}/seed-signpost.sh"; }
phase_stress() { "${OPS}/run-stress-test.sh"; }
phase_monitor(){ nohup "${OPS}/monitor-run.sh" > "${ROOT}/docs/profiling/run-2/monitor.log" 2>&1 & echo "monitor PID=$!"; }

phase_flames() {
  # Three windows: ~T+15 min, T+1h45, T+2h45 (mid-burst).
  for offset in 900 6300 9900; do
    sleep_until=$(($(date +%s) + offset))
    echo "scheduling flame at T+${offset}s"
    ( while [[ $(date +%s) -lt $sleep_until ]]; do sleep 30; done
      "${OPS}/perf-flamegraph.sh" broccoli-api-1 broccoli-server 60
      "${OPS}/perf-flamegraph.sh" broccoli-worker-1 broccoli-worker 60
    ) &
  done
  wait
}

phase_collect() { "${OPS}/collect-artifacts.sh"; }

phase_teardown() {
  "${OPS}/teardown-run2.sh"
}

phase_all() {
  phase_bootstrap
  phase_build   # blocks until the remote build finishes (fails on build error)
  phase_deploy
  phase_seed
  phase_monitor
  phase_stress &
  STRESS_PID=$!
  phase_flames
  wait "${STRESS_PID}" || true
  phase_collect
  # Manual analysis review here.
  # phase_teardown
}

case "${1:?usage: $0 {provision|bootstrap|build|build-wait|deploy|seed|stress|monitor|flames|collect|teardown|all}}" in
  provision) phase_provision ;;
  bootstrap) phase_bootstrap ;;
  build)     phase_build ;;
  build-wait) phase_build_wait ;;
  deploy)    phase_deploy ;;
  seed)      phase_seed ;;
  stress)    phase_stress ;;
  monitor)   phase_monitor ;;
  flames)    phase_flames ;;
  collect)   phase_collect ;;
  teardown)  phase_teardown ;;
  all)       phase_all ;;
  *) "$0"; exit 1 ;;
esac
