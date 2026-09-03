#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bb="$here/../build-bundle.sh"
[ -f "$bb" ] || { echo "FAIL: build-bundle.sh missing"; exit 1; }
bash -n "$bb" || { echo "FAIL: build-bundle.sh does not parse"; exit 1; }

# structural: mints CA and writes manifest + bundle.json
grep -q 'mint-ca.sh'      "$bb" || { echo "FAIL: does not mint CA"; exit 1; }
grep -q 'manifest_generate' "$bb" || { echo "FAIL: does not generate manifest"; exit 1; }
grep -q 'bundle.json'     "$bb" || { echo "FAIL: does not write bundle.json"; exit 1; }

# --skip-images assembles a tree with no docker (CI-safe)
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
bash "$bb" --version testv --output "$T" --skip-images >/dev/null
b="$T/broccoli-airgap-testv"
for p in bundle.json manifest.sha256 ca/root.crt caddy/Caddyfile.airgap \
         compose/docker-compose.gateway-airgap.yaml.template \
         load-bundle.sh install.sh setup.sh trust-ca/linux.sh compose \
         lib/manifest.sh lib/runtime.sh lib/answers.sh lib/envgen.sh \
         lib/preflight.sh native/live-boot-preflight.sh; do
  [ -e "$b/$p" ] || { echo "FAIL: bundle missing $p"; exit 1; }
done
[ -x "$b/native/live-boot-preflight.sh" ] || { echo "FAIL: staged preflight not executable"; exit 1; }
[ -x "$b/setup.sh" ] || { echo "FAIL: staged setup.sh not executable"; exit 1; }
# worker deploy files must be staged
for p in compose/docker-compose.worker.yaml.template compose/.env.worker.example; do
  [ -e "$b/$p" ] || { echo "FAIL: bundle missing $p"; exit 1; }
done
# staged env examples carry LOCAL versioned image tags so target `--pull never` resolves
grep -qx 'BROCCOLI_SERVER_IMAGE=broccoli-server:testv' "$b/compose/.env.server.example" \
  || { echo "FAIL: .env.server.example image tag not rewritten to broccoli-server:testv"; exit 1; }
grep -qx 'BROCCOLI_WORKER_IMAGE=broccoli-worker:testv' "$b/compose/.env.worker.example" \
  || { echo "FAIL: .env.worker.example image tag not rewritten to broccoli-worker:testv"; exit 1; }
# manifest actually verifies
# shellcheck source=/dev/null
source "$here/../lib/manifest.sh"; manifest_verify "$b" >/dev/null \
  || { echo "FAIL: assembled bundle fails its own manifest"; exit 1; }
# ship-clean: build-bundle must assert the assembled tree carries NO on-host env
# config (those basenames are manifest-excluded, so a leaked one would ride every
# bundle undetected) — and the assembled tree must actually be clean.
grep -q 'manifest_no_hostenv' "$bb" \
  || { echo "FAIL: build-bundle must assert ship-clean via manifest_no_hostenv"; exit 1; }
manifest_no_hostenv "$b" >/dev/null 2>&1 \
  || { echo "FAIL: assembled bundle carries on-host env config"; exit 1; }
# security: NO private key ever enters the client-distributed (manifested) tree.
for k in ca/root.key ca/server.key; do
  [ ! -e "$b/$k" ] || { echo "FAIL: private key leaked into client tree: $k"; exit 1; }
done
grep -qE '(^| )\./ca/(root|server)\.key$' "$b/manifest.sha256" \
  && { echo "FAIL: manifest lists a private key"; exit 1; } || true
# the CA/leaf private keys live in the server-only sidecar instead
[ -f "$T/broccoli-airgap-testv.server-secret/root.key" ] \
  || { echo "FAIL: server-secret sidecar missing root.key"; exit 1; }

# cluster-secret sidecar: sibling, present, has the shared machine-secret keys, NOT manifested
cls="$T/broccoli-airgap-testv.cluster-secret/cluster-secrets.env"
[ -f "$cls" ] || { echo "FAIL: cluster-secret sidecar missing cluster-secrets.env"; exit 1; }
for k in POSTGRES_PASSWORD REDIS_PASSWORD \
         BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY \
         BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY; do
  grep -qE "^${k}=" "$cls" || { echo "FAIL: cluster-secret missing $k"; exit 1; }
done
# no lan-host on this build -> no server host baked
grep -qE '^BROCCOLI_SERVER_HOST=' "$cls" && { echo "FAIL: server host baked without --lan-host"; exit 1; } || true
# leak guard: cluster secrets never appear in the manifested tree
grep -q 'cluster-secret' "$b/manifest.sha256" && { echo "FAIL: cluster-secret path leaked into manifest"; exit 1; } || true

# second build WITH --lan-host bakes BROCCOLI_SERVER_HOST
bash "$bb" --version testv2 --output "$T" --lan-host contest.lan --skip-images >/dev/null
cls2="$T/broccoli-airgap-testv2.cluster-secret/cluster-secrets.env"
grep -qx 'BROCCOLI_SERVER_HOST=contest.lan' "$cls2" \
  || { echo "FAIL: --lan-host not baked into cluster-secret as BROCCOLI_SERVER_HOST"; exit 1; }

# --- caddy image tag parse must be crash-proof under `set -euo pipefail`. The
#     tag's only source of truth is the ${CADDY_IMAGE:-...} default in the gateway
#     template; the parse must (a) use grep -m1 (no `head` closing the pipe early
#     -> SIGPIPE -> pipefail abort even on a match) and (b) tolerate a no-match
#     without aborting, so the :- fallback can supply the shipped literal. ---
grep -qE "grep -oE -m1 'CADDY_IMAGE" "$bb" \
  || { echo "FAIL: caddy tag parse must use 'grep -oE -m1' (head|pipefail SIGPIPE hazard)"; exit 1; }
grep -qE "CADDY_IMAGE:-\[\^}\]\+.*\|\| true" "$bb" \
  || { echo "FAIL: caddy tag parse must guard no-match with '|| true' (else set -e aborts before the fallback)"; exit 1; }
# behavioral: replicate the parse contract — a template with NO CADDY_IMAGE line
# must NOT abort and must fall through to the shipped default.
gw_real="$here/../../docker-compose.gateway-airgap.yaml.template"
tag_real="$( set -euo pipefail; c="$(grep -oE -m1 'CADDY_IMAGE:-[^}]+' "$gw_real" | cut -d- -f2- || true)"; echo "${c:-caddy:2-alpine}" )"
[ "$tag_real" = 'caddy:2-alpine' ] || { echo "FAIL: caddy parse on the real template got '$tag_real', want caddy:2-alpine"; exit 1; }
nomatch="$(mktemp)"; printf 'services:\n  gateway:\n    image: caddy\n' > "$nomatch"
tag_none="$( set -euo pipefail; c="$(grep -oE -m1 'CADDY_IMAGE:-[^}]+' "$nomatch" | cut -d- -f2- || true)"; echo "${c:-caddy:2-alpine}" )"
rc=$?; rm -f "$nomatch"
[ "$rc" = 0 ] || { echo "FAIL: caddy parse aborted (rc=$rc) on a template with no CADDY_IMAGE line"; exit 1; }
[ "$tag_none" = 'caddy:2-alpine' ] || { echo "FAIL: caddy parse no-match fallback got '$tag_none', want caddy:2-alpine"; exit 1; }

echo "PASS: build-bundle assembles a verifiable tree (skip-images)"
