#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
rel="$here/../.."          # release/
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

# fake bundle with compose examples + valid manifest
mkdir -p "$tmp/bundle/compose"
cp "$rel/.env.infra.example"  "$tmp/bundle/compose/.env.infra.example"
cp "$rel/.env.server.example" "$tmp/bundle/compose/.env.server.example"
echo img > "$tmp/bundle/images.txt"
( cd "$here/.." && . lib/manifest.sh && manifest_generate "$tmp/bundle" )
# server-secret sidecar (sibling of bundle) with root.key
sec="$tmp/bundle.server-secret"; mkdir -p "$sec"; echo k > "$sec/root.key"
# fake WORKING docker
mkdir -p "$tmp/bin"
cat > "$tmp/bin/docker" <<'E'
#!/usr/bin/env bash
case "$1" in info) exit 0;; compose) [ "$2" = version ] && exit 0; exit 0;; *) exit 0;; esac
E
chmod +x "$tmp/bin/docker"

out="$(PATH="$tmp/bin:$PATH" BROCCOLI_SETUP_ADMIN_PASS=adminpw123 \
  bash "$here/../setup.sh" --role server --bundle "$tmp/bundle" \
    --lan-host contest.lan --admin-user admin --server-secret "$sec" \
    --non-interactive --dry-run)"

echo "$out" | grep -q 'engine:  *docker'          || { echo "FAIL: engine not reported"; exit 1; }
echo "$out" | grep -q 'compose:  *docker compose'  || { echo "FAIL: compose not reported"; exit 1; }
echo "$out" | grep -q 'install.sh --role server'   || { echo "FAIL: exec plan missing"; exit 1; }
# env files generated with consistent secrets + service-name endpoints
sv="$tmp/bundle/compose/.env.server"
grep -qE '^BROCCOLI__DATABASE__URL=postgres://postgres:.+@db:5432/broccoli$' "$sv" || { echo "FAIL: db url"; exit 1; }
grep -q '10.0.0.10' "$sv" && { echo "FAIL: phantom IP remains"; exit 1; } || true
grep -q 'change-me' "$sv" && { echo "FAIL: placeholder remains"; exit 1; } || true
# dry-run started nothing (no compose project dir side effects beyond env files) — implicit
# dry-run must actually gate exec: install.sh's real runtime marker must NOT appear
echo "$out" | grep -q 'TLS gateway will serve' && { echo "FAIL: dry-run reached install.sh"; exit 1; } || true

# default server-secret sidecar convention (no --server-secret passed) must be
# honored by preflight too, not just by install.sh — regression guard for the
# "preflight FAILs on the default sidecar" bug
set +e
out2="$(PATH="$tmp/bin:$PATH" BROCCOLI_SETUP_ADMIN_PASS=adminpw123 \
  bash "$here/../setup.sh" --role server --bundle "$tmp/bundle" \
    --lan-host contest.lan --admin-user admin \
    --non-interactive --dry-run)"
rc2=$?
set -e
[ "$rc2" = 0 ] || { echo "FAIL: default server-secret sidecar not honored by preflight (rc=$rc2)"; echo "$out2"; exit 1; }
echo "$out2" | grep -q 'install.sh --role server' || { echo "FAIL: exec plan missing (default server-secret run)"; exit 1; }

# the DOCUMENTED invocation is `cd <bundle-dir>; ./setup.sh --bundle .` — the
# default sidecar must resolve to the SIBLING dir, not `..server-secret` inside
# the bundle (canonicalization regression guard).
set +e
out3="$(cd "$tmp/bundle" && PATH="$tmp/bin:$PATH" BROCCOLI_SETUP_ADMIN_PASS=adminpw123 \
  bash "$here/../setup.sh" --role server --bundle . \
    --lan-host contest.lan --admin-user admin \
    --non-interactive --dry-run)"
rc3=$?
set -e
[ "$rc3" = 0 ] || { echo "FAIL: --bundle . default sidecar not resolved (rc=$rc3)"; echo "$out3"; exit 1; }
echo "$out3" | grep -q 'install.sh --role server' || { echo "FAIL: --bundle . run did not reach exec plan"; echo "$out3"; exit 1; }

# missing required non-interactive answer (no lan-host) -> exit 2
set +e
PATH="$tmp/bin:$PATH" BROCCOLI_SETUP_ADMIN_PASS=x \
  bash "$here/../setup.sh" --role server --bundle "$tmp/bundle" \
    --admin-user admin --server-secret "$sec" --non-interactive --dry-run >/dev/null 2>&1
rc=$?
set -e
[ "$rc" = 2 ] || { echo "FAIL: missing --lan-host should exit 2 (rc=$rc)"; exit 1; }

echo "PASS: setup.sh dry-run wiring + config generation"
