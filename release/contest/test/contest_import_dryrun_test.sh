#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
import="$here/../import-problems.sh"

bundle="$(mktemp -d)"
trap 'rm -rf "$bundle"' EXIT
mkdir -p "$bundle/db"
printf '{"tag":"broccoli-contest/v1","kind":"contest"}\n' > "$bundle/manifest.json"
for t in role role_permission user user_role contest problem test_case contest_problem contest_user plugin_config; do
  : > "$bundle/db/$t.dat"
done

out="$(bash "$import" --bundle "$bundle" --dry-run 2>&1)"

fail=0
check() { grep -Fq -- "$1" <<<"$out" || { echo "MISSING: $1"; fail=1; }; }
# FK-safe order: role before user_role; user before user_role; contest before contest_problem/contest_user.
ord() { local a b; a=$(grep -n -- "$1" <<<"$out" | head -1 | cut -d: -f1); b=$(grep -n -- "$2" <<<"$out" | head -1 | cut -d: -f1);
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] || { echo "ORDER: '$1' must precede '$2'"; fail=1; }; }

check "INSERT INTO \"user\""
check "ON CONFLICT (username) WHERE deleted_at IS NULL DO NOTHING"
check "ON CONFLICT (contest_id, user_id) DO NOTHING"
check "setval(pg_get_serial_sequence('contest'"
ord 'INTO role'       'INTO user_role'
ord 'INTO "user"'     'INTO user_role'
ord 'INTO contest'    'INTO contest_user'
ord 'INTO contest'    'INTO contest_problem'

[ "$fail" -eq 0 ] && echo "PASS: contest import dry-run" || { echo "FAIL"; exit 1; }
