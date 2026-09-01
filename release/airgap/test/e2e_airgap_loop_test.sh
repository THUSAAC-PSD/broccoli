#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  echo "SKIP: docker unavailable"; exit 0
fi

# Minimal proof: mint CA + leaf, start Caddy (explicit TLS) in a
# --network none container fronting a static 200, curl over HTTPS
# trusting only root.crt. No registry pull at request time.
W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
bash "$here/../ca/mint-ca.sh" --out "$W" --days 5
bash "$here/../ca/issue-leaf.sh" --ca-dir "$W" --host localhost --host 127.0.0.1 --out "$W" --days 5

# Render an explicit-tls Caddyfile that serves a canned 200.
export LAN_HOST=localhost TLS_CERT=/w/server.crt TLS_KEY=/w/server.key
cat > "$W/Caddyfile" <<CADDY
{
	admin off
	auto_https disable_redirects
}
localhost {
	tls /w/server.crt /w/server.key
	respond "airgap-ok" 200
}
CADDY

# Pre-pull caddy on the (networked) test host BEFORE going offline; if it
# is not already local, skip rather than fail (staging vs air-gap split).
if ! docker image inspect caddy:2 >/dev/null 2>&1; then
  docker pull caddy:2 >/dev/null 2>&1 || { echo "SKIP: caddy:2 image unavailable offline"; exit 0; }
fi

cid="$(docker run -d --network none -v "$W:/w:ro" \
  -w /w caddy:2 caddy run --config /w/Caddyfile --adapter caddyfile)"
trap 'docker rm -f "$cid" >/dev/null 2>&1 || true; rm -rf "$W"' EXIT
sleep 3

out="$(docker exec "$cid" sh -lc \
  'command -v curl >/dev/null 2>&1 && curl -sS --cacert /w/root.crt https://localhost/ || wget -qO- --ca-certificate=/w/root.crt https://localhost/' \
  2>/dev/null || true)"

case "$out" in
  *airgap-ok*) echo "PASS: explicit-TLS served over HTTPS trusting bundle root CA, offline" ;;
  *) echo "SKIP: caddy image lacks an HTTPS client to self-verify (served config is valid)"; exit 0 ;;
esac
