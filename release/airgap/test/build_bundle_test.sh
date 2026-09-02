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

echo "PASS: build-bundle assembles a verifiable tree (skip-images)"
