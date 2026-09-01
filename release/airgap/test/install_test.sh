#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ins="$here/../install.sh"
[ -f "$ins" ] || { echo "FAIL: install.sh missing"; exit 1; }
bash -n "$ins" || { echo "FAIL: install.sh does not parse"; exit 1; }

# dispatches all three roles + rejects unknown
grep -q 'server)'      "$ins" || { echo "FAIL: no server role"; exit 1; }
grep -q 'worker)'      "$ins" || { echo "FAIL: no worker role"; exit 1; }
grep -q 'contestant)'  "$ins" || { echo "FAIL: no contestant role"; exit 1; }

# renders the airgap Caddyfile and honors --pull never
grep -q 'Caddyfile.airgap' "$ins" || { echo "FAIL: does not use Caddyfile.airgap"; exit 1; }
grep -q -- '--pull never'  "$ins" || { echo "FAIL: does not pass --pull never"; exit 1; }

# offline invariant (hardened: word-anchored pattern)
grep -Eq '\b(curl|wget|apt|apt-get|pip[0-9]*)\b' "$ins" && { echo "FAIL: network fetch present"; exit 1; }
grep -q 'docker pull' "$ins" && { echo "FAIL: docker pull present"; exit 1; }

# unknown role exits nonzero
if bash "$ins" --role bogus --bundle /tmp/nope 2>/dev/null; then
  echo "FAIL: unknown role accepted"; exit 1
fi
echo "PASS: install.sh dispatches roles, renders airgap Caddyfile, offline"
