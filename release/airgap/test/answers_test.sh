#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
lib="$here/../lib/answers.sh"

# flag beats env
out="$(FLAG_LAN_HOST=flaghost BROCCOLI_SETUP_LAN_HOST=envhost FLAG_NON_INTERACTIVE=1 \
       bash -c '. '"$lib"'; answer LAN_HOST prompt "" 1')"
[ "$out" = flaghost ] || { echo "FAIL: flag>env (got $out)"; exit 1; }

# env used when no flag
out="$(BROCCOLI_SETUP_LAN_HOST=envhost FLAG_NON_INTERACTIVE=1 bash -c '. '"$lib"'; answer LAN_HOST prompt "" 1')"
[ "$out" = envhost ] || { echo "FAIL: env fallback (got $out)"; exit 1; }

# default used non-interactively when nothing set
out="$(FLAG_NON_INTERACTIVE=1 bash -c '. '"$lib"'; answer ROLE prompt server 0')"
[ "$out" = server ] || { echo "FAIL: default (got $out)"; exit 1; }

# required + unset non-interactive -> exit 2
set +e
( FLAG_NON_INTERACTIVE=1 bash -c '. '"$lib"'; answer LAN_HOST prompt "" 1' ) >/dev/null 2>&1
rc=$?
set -e
[ "$rc" = 2 ] || { echo "FAIL: required-missing should exit 2 (got $rc)"; exit 1; }

# answer_secret: env value returned, no prompt in non-interactive
out="$(BROCCOLI_SETUP_ADMIN_PASS=s3cret FLAG_NON_INTERACTIVE=1 bash -c '. '"$lib"'; answer_secret ADMIN_PASS prompt')"
[ "$out" = s3cret ] || { echo "FAIL: answer_secret env (got $out)"; exit 1; }

echo "PASS: answers precedence + non-interactive"
