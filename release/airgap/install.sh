#!/usr/bin/env bash
# Role dispatcher for an air-gapped broccoli LAN install. TARGET-SIDE:
# no network. All images come from the bundle; compose runs --pull never.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"

usage() {
  echo "Usage: install.sh --role {server|worker|contestant} --bundle DIR [--lan-host H] [--burn-ca-key]"
}

ROLE="" BUNDLE="" LAN_HOST="" BURN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --role) ROLE="$2"; shift 2 ;;
    --bundle) BUNDLE="$2"; shift 2 ;;
    --lan-host) LAN_HOST="$2"; shift 2 ;;
    --burn-ca-key) BURN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done
[ -n "$ROLE" ] && [ -n "$BUNDLE" ] || { echo "--role and --bundle are required" >&2; usage; exit 2; }

os_helper() {
  case "$(uname -s)" in
    Linux)  echo "$here/trust-ca/linux.sh" ;;
    Darwin) echo "$here/trust-ca/macos.sh" ;;
    *) echo "" ;;
  esac
}

case "$ROLE" in
  server)
    # Validate the host BEFORE any heavy work (bundle load, leaf issuance).
    [ -n "$LAN_HOST" ] || { echo "server role requires --lan-host" >&2; exit 2; }
    # Secrets are operator-supplied: the bundle ships .env.*.example only.
    # compose substitutes ${VARS} from its shell env, so the real env files
    # must exist and be passed via --env-file or every var expands empty.
    infra_env="$BUNDLE/compose/.env.infra"
    server_env="$BUNDLE/compose/.env.server"
    for f in "$infra_env" "$server_env"; do
      [ -f "$f" ] || { echo "missing ${f} — copy ${f}.example to ${f} and fill in secrets (see release/docs/airgap-deployment.md)" >&2; exit 2; }
    done
    bash "$here/load-bundle.sh" --bundle "$BUNDLE"
    if [ ! -f "$BUNDLE/ca/server.crt" ]; then
      bash "$here/ca/issue-leaf.sh" --ca-dir "$BUNDLE/ca" --host "$LAN_HOST" --out "$BUNDLE/ca"
    fi
    export LAN_HOST TLS_CERT="$BUNDLE/ca/server.crt" TLS_KEY="$BUNDLE/ca/server.key"
    export BROCCOLI_UPSTREAMS="${BROCCOLI_UPSTREAMS:-server:3000}"
    envsubst < "$here/caddy/Caddyfile.airgap" > "$BUNDLE/caddy/Caddyfile"
    echo "rendered $BUNDLE/caddy/Caddyfile for $LAN_HOST"
    if [ "$BURN" = "1" ]; then rm -f "$BUNDLE/ca/root.key"; echo "burned ca/root.key"; fi
    ( cd "$BUNDLE/compose" && docker compose \
        --env-file .env.infra --env-file .env.server \
        -f docker-compose.infra.yaml.template \
        -f docker-compose.server.yaml.template up -d --pull never )
    ;;
  worker)
    bash "$here/load-bundle.sh" --bundle "$BUNDLE"
    # build-bundle.sh (Task 8) stages the preflight into the bundle at native/.
    preflight="$here/native/live-boot-preflight.sh"
    if [ -x "$preflight" ]; then
      bash "$preflight" || echo "WARN: worker sandbox preflight reported issues"
    else
      echo "WARN: sandbox preflight not found at $preflight — skipping go/no-go check" >&2
    fi
    # Surface a genuine trust failure; only unsupported-OS is a soft warning.
    helper="$(os_helper)"
    if [ -n "$helper" ]; then
      bash "$helper" "$BUNDLE/ca/root.crt" || { echo "ERROR: CA trust failed" >&2; exit 1; }
    else
      echo "WARN: unsupported OS for CA trust helper; trust $BUNDLE/ca/root.crt manually" >&2
    fi
    echo "worker installed"
    ;;
  contestant)
    # Re-verify bundle integrity before trusting a CA or installing a binary
    # onto PATH — this is the client-side trust boundary after USB transfer.
    bash "$here/load-bundle.sh" --bundle "$BUNDLE" --verify-only
    helper="$(os_helper)"
    [ -n "$helper" ] || { echo "unsupported OS for contestant trust helper" >&2; exit 1; }
    bash "$helper" "$BUNDLE/ca/root.crt"
    if [ -f "$BUNDLE/cli/broccoli" ]; then
      sudo install -m 0755 "$BUNDLE/cli/broccoli" /usr/local/bin/broccoli
      echo "installed contestant CLI -> /usr/local/bin/broccoli"
    fi
    ;;
  *) echo "unknown role: $ROLE" >&2; usage; exit 2 ;;
esac
