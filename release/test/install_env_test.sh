#!/usr/bin/env bash
# release/install.sh dry runs: values passed at install time reach every env
# file, each role gets its own Compose project, and BROCCOLI_PLUGINS enables
# optional plugins.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
cp -r "$here/../." "$T/"
cd "$T"
mkdir -p plugins plugins-available/codelink-bracket
printf 'name = "codelink-bracket"\n' > plugins-available/codelink-bracket/plugin.toml

fail() { echo "FAIL: $*"; exit 1; }
run() { BROCCOLI_DRY_RUN=1 BROCCOLI_SKIP_TIME_CHECK=1 ./install.sh "$@" </dev/null >/dev/null 2>&1; }

BROCCOLI__AUTH__SECURE_COOKIES=true \
BROCCOLI__STORAGE__OBJECT_STORAGE__REGION=cn-north-1 \
BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE=false \
  run infra || fail "infra dry run"
for f in .env.infra server-secrets.env; do
  grep -qx "BROCCOLI__AUTH__SECURE_COOKIES='true'" "$f" || fail "$f ignored SECURE_COOKIES"
done
for f in .env.infra connection.env; do
  grep -qx "BROCCOLI__STORAGE__OBJECT_STORAGE__REGION='cn-north-1'" "$f" || fail "$f ignored REGION"
  grep -qx "BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE='false'" "$f" || fail "$f ignored PATH_STYLE"
done
grep -qx "COMPOSE_PROJECT_NAME='broccoli-infra'" .env.infra || fail "infra has no project name"

BROCCOLI_PLUGINS="codelink-bracket" run server || fail "server dry run"
grep -qx "COMPOSE_PROJECT_NAME='broccoli-server'" .env.server || fail "server has no project name"
grep -qx "BROCCOLI__AUTH__SECURE_COOKIES='true'" .env.server || fail "server lost SECURE_COOKIES"
[ -f plugins/codelink-bracket/plugin.toml ] || fail "BROCCOLI_PLUGINS did not enable codelink-bracket"

rm -f .env.worker
if BROCCOLI_PLUGINS="no-such-plugin" run worker; then
  fail "an unknown plugin in BROCCOLI_PLUGINS was accepted"
fi
echo PASS
