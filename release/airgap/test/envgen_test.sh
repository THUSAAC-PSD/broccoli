#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../lib/envgen.sh"
rel="$here/../.."          # release/
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
infra="$tmp/.env.infra"; server="$tmp/.env.server"

envgen_write "$infra" "$server" \
  "$rel/.env.infra.example" "$rel/.env.server.example" \
  admin 'adminpw123'

# postgres password identical in infra and the server DB URL
pg="$(grep -E '^POSTGRES_PASSWORD=' "$infra" | cut -d= -f2-)"
[ -n "$pg" ] || { echo "FAIL: empty pg password"; exit 1; }
grep -qE "^BROCCOLI__DATABASE__URL=postgres://postgres:${pg}@db:5432/broccoli$" "$server" \
  || { echo "FAIL: DB URL password/host mismatch"; exit 1; }
# endpoints default to service names; phantom IP gone
grep -qE '^BROCCOLI__MQ__URL=redis://:.+@redis:6379$' "$server" || { echo "FAIL: MQ endpoint"; exit 1; }
grep -qE '^BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=http://seaweedfs:8333$' "$server" || { echo "FAIL: S3 endpoint"; exit 1; }
grep -q '10.0.0.10' "$server" && { echo "FAIL: phantom 10.0.0.10 remains"; exit 1; } || true
# infra file also has service-name endpoint, no phantom IP
grep -qE '^BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=http://seaweedfs:8333$' "$infra" || { echo "FAIL: infra S3 endpoint"; exit 1; }
! grep -q '10.0.0.10' "$infra" || { echo "FAIL: phantom 10.0.0.10 in infra"; exit 1; }
# S3 keys consistent across both files
for k in BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY; do
  a="$(grep -E "^$k=" "$infra" | cut -d= -f2-)"; b="$(grep -E "^$k=" "$server" | cut -d= -f2-)"
  [ -n "$a" ] && [ "$a" = "$b" ] || { echo "FAIL: $k inconsistent"; exit 1; }
done
# JWT >= 32 chars
jwt="$(grep -E '^BROCCOLI__AUTH__JWT_SECRET=' "$server" | cut -d= -f2-)"
[ "${#jwt}" -ge 32 ] || { echo "FAIL: jwt too short (${#jwt})"; exit 1; }
# admin creds written
grep -qE '^BROCCOLI_BOOTSTRAP_ADMIN_USERNAME=admin$' "$server" || { echo "FAIL: admin user"; exit 1; }
grep -qE '^BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD=adminpw123$' "$server" || { echo "FAIL: admin pass"; exit 1; }
# no change-me placeholder in either file
grep -q 'change-me' "$infra"  && { echo "FAIL: placeholder in infra"; exit 1; } || true
grep -q 'change-me' "$server" && { echo "FAIL: placeholder in server"; exit 1; } || true

# backslash escape: env_set replace branch must not corrupt values with backslash sequences
# (the bug lived in the awk -v replace path, not append). Test both paths on same file.
esc_test="$tmp/.env.esc"
env_set "$esc_test" TESTKEY 'C:\temp\new'    # append (creates the key)
env_set "$esc_test" TESTKEY 'C:\temp\new2'   # replace — the path that was broken by awk -v
esc_lines="$(grep -c '^TESTKEY=' "$esc_test")"
[ "$esc_lines" = "1" ] || { echo "FAIL: backslash replace split the line"; exit 1; }
esc_val="$(env_get "$esc_test" TESTKEY)"
[ "$esc_val" = 'C:\temp\new2' ] || { echo "FAIL: backslash value round-trip mismatch: $esc_val"; exit 1; }

# idempotent: rerun with same args preserves every byte
cp "$infra" "$tmp/i1"; cp "$server" "$tmp/s1"
envgen_write "$infra" "$server" \
  "$rel/.env.infra.example" "$rel/.env.server.example" admin 'adminpw123'
diff -q "$tmp/i1" "$infra"  >/dev/null || { echo "FAIL: infra not idempotent"; exit 1; }
diff -q "$tmp/s1" "$server" >/dev/null || { echo "FAIL: server not idempotent"; exit 1; }

# --- fixture: a cluster-secret sidecar ---
cls="$tmp/cluster-secrets.env"
cat > "$cls" <<EOF
POSTGRES_PASSWORD=pgpw
REDIS_PASSWORD=rdpw
BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY=s3acc
BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY=s3sec
BROCCOLI_SERVER_HOST=10.0.0.10
EOF

# --- workergen_write renders correct URLs/creds/host/id ---
wex="$rel/.env.worker.example"
w="$tmp/.env.worker"
workergen_write "$w" "$wex" "$cls" "10.0.0.10" "worker-7"
grep -qx 'BROCCOLI__DATABASE__URL=postgres://postgres:pgpw@10.0.0.10:5432/broccoli' "$w" \
  || { echo "FAIL: worker DATABASE_URL wrong"; cat "$w"; exit 1; }
grep -qx 'BROCCOLI__MQ__URL=redis://:rdpw@10.0.0.10:6379' "$w" \
  || { echo "FAIL: worker MQ_URL wrong"; exit 1; }
grep -qx 'BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=http://10.0.0.10:8333' "$w" \
  || { echo "FAIL: worker S3 endpoint wrong"; exit 1; }
grep -qx 'BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY=s3acc' "$w" || { echo "FAIL: worker S3 access wrong"; exit 1; }
grep -qx 'BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY=s3sec' "$w" || { echo "FAIL: worker S3 secret wrong"; exit 1; }
grep -qx 'BROCCOLI__WORKER__ID=worker-7' "$w" || { echo "FAIL: worker id wrong"; exit 1; }

# --- workergen_write refuses a missing server host ---
if workergen_write "$tmp/.env.worker2" "$wex" "$cls" "" "worker-1" 2>/dev/null; then
  echo "FAIL: workergen accepted empty server host"; exit 1; fi

# --- cluster_seed_infra makes envgen_write REUSE the sidecar secrets ---
iex="$rel/.env.infra.example"; sex="$rel/.env.server.example"
infra2="$tmp/.env.infra2"; server2="$tmp/.env.server2"
cp "$iex" "$infra2"                       # start from example (has change-me placeholders)
cluster_seed_infra "$infra2" "$cls"
envgen_write "$infra2" "$server2" "$iex" "$sex" "admin" "adminpw"
grep -qx 'POSTGRES_PASSWORD=pgpw' "$infra2" || { echo "FAIL: infra did not reuse sidecar PG pw"; exit 1; }
grep -qx 'REDIS_PASSWORD=rdpw' "$infra2"    || { echo "FAIL: infra did not reuse sidecar redis pw"; exit 1; }
grep -q  'postgres://postgres:pgpw@' "$server2" || { echo "FAIL: server DATABASE_URL did not use sidecar PG pw"; exit 1; }

echo "PASS: envgen secrets consistent, endpoints service-named, idempotent, cluster-seed + workergen_write"
