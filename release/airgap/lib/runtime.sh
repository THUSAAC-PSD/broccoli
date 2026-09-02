#!/usr/bin/env bash
# Container-runtime abstraction shared by setup.sh (UX) and install.sh (engine).
# Detects a WORKING docker or podman, resolves the compose invocation, and
# relabels bind mounts for rootless Podman under SELinux. TARGET-SIDE: no network.
set -euo pipefail

# Echo the engine whose `<engine> info` works: docker preferred, then podman.
# Honors $BROCCOLI_ENGINE as an explicit override (must itself work, else "").
runtime_engine() {
  local override="${BROCCOLI_ENGINE:-}" e
  if [ -n "$override" ]; then
    if command -v "$override" >/dev/null 2>&1 && "$override" info >/dev/null 2>&1; then
      echo "$override"
    else
      echo ""
    fi
    return 0
  fi
  for e in docker podman; do
    if command -v "$e" >/dev/null 2>&1 && "$e" info >/dev/null 2>&1; then
      echo "$e"; return 0
    fi
  done
  echo ""
}

# Echo a working compose invocation for the given engine, or "" if none.
runtime_compose() {
  local engine="$1"
  case "$engine" in
    docker)
      if docker compose version >/dev/null 2>&1; then echo "docker compose"; return 0; fi
      if command -v docker-compose >/dev/null 2>&1 && docker-compose version >/dev/null 2>&1; then
        echo "docker-compose"; return 0
      fi ;;
    podman)
      if podman compose version >/dev/null 2>&1; then echo "podman compose"; return 0; fi
      if command -v podman-compose >/dev/null 2>&1 && podman-compose version >/dev/null 2>&1; then
        echo "podman-compose"; return 0
      fi ;;
  esac
  echo ""
}

# rc 0 iff SELinux is Enforcing (guarded by presence of getenforce).
runtime_selinux_enforcing() {
  command -v getenforce >/dev/null 2>&1 || return 1
  [ "$(getenforce 2>/dev/null)" = "Enforcing" ]
}

# Relabel bind-mount sources for rootless Podman under SELinux. Best-effort.
# Args: engine then paths. No-op unless engine==podman AND SELinux Enforcing.
runtime_relabel() {
  local engine="$1"; shift
  [ "$engine" = "podman" ] || return 0
  runtime_selinux_enforcing || return 0
  local p
  for p in "$@"; do
    [ -e "$p" ] || continue
    chcon -Rt container_file_t "$p" 2>/dev/null || echo "WARN: SELinux relabel failed for $p" >&2
  done
}
