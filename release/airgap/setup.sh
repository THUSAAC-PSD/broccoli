#!/usr/bin/env bash
# One-click guided installer for an air-gapped broccoli LAN. Interactive by
# default; fully flag/env-drivable (--non-interactive) for tests and a future
# Ansible slice. TARGET-SIDE: no network. Wraps install.sh (the deploy engine).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
. "$here/lib/runtime.sh"
# shellcheck source=/dev/null
. "$here/lib/answers.sh"
# shellcheck source=/dev/null
. "$here/lib/envgen.sh"
# shellcheck source=/dev/null
. "$here/lib/preflight.sh"

usage() {
  echo "Usage: setup.sh --role {server|worker|contestant} --bundle DIR [--lan-host H]" \
       "[--admin-user U] [--admin-pass P] [--engine docker|podman] [--server-secret DIR]" \
       "[--non-interactive] [--dry-run]"
}

FLAG_NON_INTERACTIVE="" DRY_RUN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --role)          FLAG_ROLE="$2"; shift 2 ;;
    --bundle)        FLAG_BUNDLE="$2"; shift 2 ;;
    --lan-host)      FLAG_LAN_HOST="$2"; shift 2 ;;
    --admin-user)    FLAG_ADMIN_USER="$2"; shift 2 ;;
    --admin-pass)    FLAG_ADMIN_PASS="$2"; shift 2 ;;
    --engine)        FLAG_ENGINE="$2"; shift 2 ;;
    --server-secret) FLAG_SERVER_SECRET="$2"; shift 2 ;;
    --non-interactive) FLAG_NON_INTERACTIVE=1; shift ;;
    --dry-run)       DRY_RUN=1; shift ;;
    -h|--help)       usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
export FLAG_NON_INTERACTIVE

ROLE="$(answer ROLE 'Role (server/worker/contestant)' '' 1)"
BUNDLE="$(answer BUNDLE 'Bundle dir' '' 1)"
[ -d "$BUNDLE" ] || { echo "bundle dir not found: $BUNDLE" >&2; exit 2; }
# Default server-secret sidecar convention (documented in
# release/docs/airgap-deployment.md, auto-resolved by install.sh): compute it
# once here so preflight and install.sh agree on the same directory.
secret_dir="${FLAG_SERVER_SECRET:-${BUNDLE%/}.server-secret}"

[ -n "${FLAG_ENGINE:-}" ] && export BROCCOLI_ENGINE="$FLAG_ENGINE"
ENGINE="$(runtime_engine)"
[ -n "$ENGINE" ] || { echo "FAIL: no working docker or podman (install Docker, or Podman on RHEL-family). Detect-and-guide only; runtime is not bundled." >&2; exit 2; }
COMPOSE="$(runtime_compose "$ENGINE")"
[ -n "$COMPOSE" ] || { echo "FAIL: '$ENGINE' present but no compose provider found." >&2; exit 2; }
export BROCCOLI_ENGINE="$ENGINE" COMPOSE

echo "== preflight ($ROLE) =="
preflight_run "$ROLE" "$BUNDLE" "$secret_dir" \
  || { echo "preflight FAILED — resolve the FAIL lines above; nothing deployed." >&2; exit 2; }

INST=( --role "$ROLE" --bundle "$BUNDLE" )

if [ "$ROLE" = server ]; then
  LAN_HOST="$(answer LAN_HOST 'LAN hostname for TLS (e.g. contest.lan)' '' 1)"
  ADMIN_USER="$(answer ADMIN_USER 'Bootstrap admin username' admin 0)"
  ADMIN_PASS="$(answer_secret ADMIN_PASS 'Bootstrap admin password')"
  infra="$BUNDLE/compose/.env.infra"; server="$BUNDLE/compose/.env.server"
  envgen_write "$infra" "$server" \
    "$BUNDLE/compose/.env.infra.example" "$BUNDLE/compose/.env.server.example" \
    "$ADMIN_USER" "$ADMIN_PASS"
  # SELinux relabel for rootless podman bind mounts (TLS dir + Caddyfile).
  runtime_relabel "$ENGINE" "$secret_dir" "$here/caddy/Caddyfile.airgap" 2>/dev/null || true
  INST+=( --lan-host "$LAN_HOST" )
  [ -n "${FLAG_SERVER_SECRET:-}" ] && INST+=( --server-secret "$FLAG_SERVER_SECRET" )
fi

if [ -n "$DRY_RUN" ]; then
  echo "== plan (dry-run) =="
  echo "engine:  $ENGINE"
  echo "compose: $COMPOSE"
  [ "$ROLE" = server ] && echo "env:     $BUNDLE/compose/.env.infra, .env.server (generated)"
  echo "exec:    install.sh ${INST[*]}"
  exit 0
fi

exec bash "$here/install.sh" "${INST[@]}"
