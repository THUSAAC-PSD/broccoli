#!/usr/bin/env bash
# Single entry point for a Broccoli profiling run.
# Once the cluster is provisioned, every step is automated:
#   ./ops/profile-runbook.sh provision  # spin up 10 deploy + 1 build droplet
#   ./ops/profile-runbook.sh bootstrap  # apt/docker/ufw on all hosts
#   ./ops/profile-runbook.sh build      # rsync source + docker build images
#   ./ops/profile-runbook.sh deploy     # distribute images & bring up services
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

phase_build() {
  rsync -avzP --exclude='target/' --exclude='node_modules/' --exclude='.git/' \
    --exclude='dist/' --exclude='qa-evidence/' --exclude='ops/logs/' \
    --exclude='ops/secrets/' --exclude='.DS_Store' \
    -e "ssh -F ${OPS}/ssh_config" "${ROOT}/" broccoli-build:/root/broccoli/
  scp -F "${OPS}/ssh_config" -q "${OPS}/remote-build.sh" broccoli-build:/root/remote-build.sh
  ssh -F "${OPS}/ssh_config" broccoli-build 'nohup bash /root/remote-build.sh > /root/build.log 2>&1 & echo "build PID=$!"'
  echo "Tail with: ssh -F ${OPS}/ssh_config broccoli-build 'tail -f /root/broccoli-dist/build-logs/build.log'"
}

phase_deploy() { "${OPS}/deploy-run2.sh" all; }
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
  phase_build; echo "Build is async; wait until /root/broccoli-dist/images/server.tar.gz exists before deploy."
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

case "${1:?usage: $0 {provision|bootstrap|build|deploy|seed|stress|monitor|flames|collect|teardown|all}}" in
  provision) phase_provision ;;
  bootstrap) phase_bootstrap ;;
  build)     phase_build ;;
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
