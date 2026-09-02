#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
inst="$here/../install.sh"

# sources the shared runtime lib (DRY: no duplicate detection)
grep -q 'lib/runtime.sh' "$inst" || { echo "FAIL: install.sh does not source lib/runtime.sh"; exit 1; }
# resolves COMPOSE, reusing an export if present
grep -qE 'COMPOSE="\$\{COMPOSE:-\$\(runtime_compose' "$inst" \
  || { echo "FAIL: install.sh does not resolve COMPOSE via runtime_compose"; exit 1; }
# uses $COMPOSE (word-split, unquoted) for the compose up
grep -qE '\$COMPOSE \\?$|&& \$COMPOSE ' "$inst" \
  || { echo "FAIL: install.sh does not invoke \$COMPOSE"; exit 1; }
# no hardcoded 'docker compose' literal remains anywhere (no tech debt)
grep -q 'docker compose' "$inst" && { echo "FAIL: hardcoded 'docker compose' literal remains"; exit 1; } || true
echo "PASS: install.sh migrated to \$COMPOSE"
