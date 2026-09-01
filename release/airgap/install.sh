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
    bash "$here/load-bundle.sh" --bundle "$BUNDLE"
    [ -n "$LAN_HOST" ] || { echo "server role requires --lan-host" >&2; exit 2; }
    if [ ! -f "$BUNDLE/ca/server.crt" ]; then
      bash "$here/ca/issue-leaf.sh" --ca-dir "$BUNDLE/ca" --host "$LAN_HOST" --out "$BUNDLE/ca"
    fi
    export LAN_HOST TLS_CERT="$BUNDLE/ca/server.crt" TLS_KEY="$BUNDLE/ca/server.key"
    export BROCCOLI_UPSTREAMS="${BROCCOLI_UPSTREAMS:-server:3000}"
    envsubst < "$here/caddy/Caddyfile.airgap" > "$BUNDLE/caddy/Caddyfile"
    echo "rendered $BUNDLE/caddy/Caddyfile for $LAN_HOST"
    if [ "$BURN" = "1" ]; then rm -f "$BUNDLE/ca/root.key"; echo "burned ca/root.key"; fi
    ( cd "$BUNDLE/compose" && docker compose \
        -f docker-compose.infra.yaml.template \
        -f docker-compose.server.yaml.template up -d --pull never )
    ;;
  worker)
    bash "$here/load-bundle.sh" --bundle "$BUNDLE"
    if [ -x "$here/../native/live-boot-preflight.sh" ]; then
      bash "$here/../native/live-boot-preflight.sh" || echo "WARN: worker sandbox preflight reported issues"
    fi
    helper="$(os_helper)"
    [ -n "$helper" ] && bash "$helper" "$BUNDLE/ca/root.crt" || true
    echo "worker installed"
    ;;
  contestant)
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
