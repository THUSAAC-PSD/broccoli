#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
export="$here/../export-problems.sh"

out="$(bash "$export" --contest 7 --dry-run 2>&1)"

fail=0
check() { grep -Fq -- "$1" <<<"$out" || { echo "MISSING: $1"; fail=1; }; }

# Contest-scoped selection and the new tables must all appear.
check "FROM contest WHERE id = 7"
check "FROM contest_problem WHERE contest_id = 7"
check "FROM contest_user WHERE contest_id = 7"
check "FROM user_role WHERE user_id IN"
check "manifest kind=contest"
# Without --with-secrets the password column must be blanked, never selected raw.
check "'' AS password"
if grep -Eq "SELECT[^;]*\\bpassword\\b[^;]*FROM \"?user\"?" <<<"$out" \
   && ! grep -Fq "'' AS password" <<<"$out"; then
  echo "LEAK: raw password selected without --with-secrets"; fail=1
fi

[ "$fail" -eq 0 ] && echo "PASS: contest export dry-run" || { echo "FAIL"; exit 1; }

# --with-secrets: the complement of the leak-guard above -- the user
# projection must select the RAW password column, never the blanked literal.
out2="$(bash "$export" --contest 7 --with-secrets --dry-run 2>&1)"
if ! grep -Eq "SELECT[^;]*\\bpassword\\b[^;]*FROM \"?user\"?" <<<"$out2"; then
  echo "MISSING: raw password column selected with --with-secrets"; fail=1
fi
if grep -Fq "'' AS password" <<<"$out2"; then
  echo "LEAK: password blanked even with --with-secrets"; fail=1
fi

[ "$fail" -eq 0 ] && echo "PASS: contest export --with-secrets dry-run" || { echo "FAIL"; exit 1; }
