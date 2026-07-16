#!/usr/bin/env bash
# Seeds an ICPC contest with the Signpost problem (qa-evidence/Signpost data).
# Runs from local mac, uploading via build droplet -> gateway:80 (VPC).
#
# Output: writes /opt/broccoli/stress-test/run-state.env on loadgen with
#   ADMIN_TOKEN, CONTEST_ID, PROBLEM_ID - to be source'd by run-stress-test.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_CFG="${ROOT}/ops/ssh_config"
SECRETS="${ROOT}/ops/secrets/profiling.env"
source "${SECRETS}"

# All API calls go through gateway (the public load-balanced entrypoint).
# Bind to gateway's private IP from inside the VPC; do this on build droplet.

log() { printf "\033[1;36m[%s]\033[0m %s\n" "$(date -u +%H:%M:%S)" "$*"; }

log "Building signpost.zip on broccoli-build..."
ssh -F "${SSH_CFG}" broccoli-build bash <<'REMOTE'
set -euo pipefail
SIGN=/root/signpost
ZIP=/root/signpost.zip
WORK=/root/signpost-zip-work
rm -rf "${WORK}" "${ZIP}"
mkdir -p "${WORK}"
# Use case-insensitive lex sort; pad to 02 digits so positions stay deterministic.
i=0
for n in $(seq 1 20); do
  pad=$(printf "%02d" "${n}")
  cp "${SIGN}/input-${n}.txt"  "${WORK}/${pad}.in"
  cp "${SIGN}/answer-${n}.txt" "${WORK}/${pad}.ans"
done
( cd "${WORK}" && zip -1 -r "${ZIP}" . >/dev/null )
ls -lh "${ZIP}"
echo "zip-ready"
REMOTE

log "Triggering API: login, create contest, create problem, upload tests..."

ssh -F "${SSH_CFG}" broccoli-build \
  ADMIN_USERNAME="${ADMIN_USERNAME}" \
  ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
  bash <<'REMOTE'
set -euo pipefail
GW=http://10.104.0.14
API="${GW}/api/v1"

ADMIN_USERNAME="${ADMIN_USERNAME:?}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:?}"

apt-get install -y -qq jq zip >/dev/null

# --- login ---
echo "logging in as ${ADMIN_USERNAME}..."
LOGIN=$(curl -sS -X POST -H 'Content-Type: application/json' \
  -d "{\"username\":\"${ADMIN_USERNAME}\",\"password\":\"${ADMIN_PASSWORD}\"}" \
  "${API}/auth/login")
TOKEN=$(echo "${LOGIN}" | jq -r '.token // .access_token // empty')
[[ -n "${TOKEN}" ]] || { echo "login failed: ${LOGIN}" >&2; exit 1; }
echo "  got token (${#TOKEN} chars)"

# --- discover plugin registries ---
REG=$(curl -sS -H "Authorization: Bearer ${TOKEN}" "${API}/plugins/registries")
CONTEST_TYPE=$(echo "${REG}" | jq -r '.contest_types[] | select(. == "icpc")' | head -1)
PROBLEM_TYPE=$(echo "${REG}" | jq -r '.problem_types[] | select(. == "batch")' | head -1)
[[ -n "${CONTEST_TYPE:-}" ]] || { echo "no icpc contest type"; echo "${REG}" | jq '.contest_types'; exit 1; }
[[ -n "${PROBLEM_TYPE:-}" ]] || { echo "no batch problem type"; echo "${REG}" | jq '.problem_types'; exit 1; }
echo "  contest_type=${CONTEST_TYPE} problem_type=${PROBLEM_TYPE}"

# --- create contest ---
# Contest starts NOW (5 s buffer) and runs for 6 hours, so the stress-test mixed
# profile sees an active contest the whole time.
START=$(date -u -d '+5 seconds' +%FT%TZ)
END=$(date -u -d '+6 hours' +%FT%TZ)
CONTEST=$(curl -sS -X POST -H "Authorization: Bearer ${TOKEN}" -H 'Content-Type: application/json' \
  -d "{\"title\":\"Signpost Profile Run 2\",\"description\":\"Mixed-mode profiling with Signpost (419 MB testcases)\",\"contest_type\":\"${CONTEST_TYPE}\",\"start_time\":\"${START}\",\"end_time\":\"${END}\",\"is_public\":true,\"submissions_visible\":true,\"show_compile_output\":true,\"show_participants_list\":true}" \
  "${API}/contests")
CONTEST_ID=$(echo "${CONTEST}" | jq -r '.id // empty')
[[ -n "${CONTEST_ID}" ]] || { echo "contest create failed: ${CONTEST}"; exit 1; }
echo "  contest_id=${CONTEST_ID}"

# --- create problem ---
PROBLEM=$(curl -sS -X POST -H "Authorization: Bearer ${TOKEN}" -H 'Content-Type: application/json' \
  -d "{\"title\":\"signpost\",\"content\":\"Find the number of valid signpost orientations. Print the count modulo 10^9+7.\",\"problem_type\":\"${PROBLEM_TYPE}\",\"default_contest_type\":\"${CONTEST_TYPE}\",\"time_limit\":3000,\"memory_limit\":262144,\"checker_format\":\"tokens\",\"show_test_details\":false}" \
  "${API}/problems")
PROBLEM_ID=$(echo "${PROBLEM}" | jq -r '.id // empty')
[[ -n "${PROBLEM_ID}" ]] || { echo "problem create failed: ${PROBLEM}"; exit 1; }
echo "  problem_id=${PROBLEM_ID}"

# --- upload test cases (419 MB ZIP) ---
# Zip layout produced earlier: 01.in / 01.ans through 20.in / 20.ans (no subfolder).
# strategy=abort: refuse if a test case already exists (clean slate here).
echo "  uploading signpost.zip (size: $(du -h /root/signpost.zip | awk '{print $1}'))..."
UPLOAD=$(curl -sS -X POST -H "Authorization: Bearer ${TOKEN}" \
  -F "file=@/root/signpost.zip;type=application/zip" \
  -F "input_format=*.in" \
  -F "output_format=*.ans" \
  -F "strategy=abort" \
  "${API}/problems/${PROBLEM_ID}/test-cases/upload")
TC_COUNT=$(echo "${UPLOAD}" | jq -r '.total // .created // length // 0' 2>/dev/null || echo 0)
echo "  uploaded test cases: ${TC_COUNT}"
echo "${UPLOAD}" | jq -c . | head -1

# --- attach to contest ---
ATTACH=$(curl -sS -X POST -H "Authorization: Bearer ${TOKEN}" -H 'Content-Type: application/json' \
  -d "{\"problem_id\":${PROBLEM_ID},\"label\":\"A\"}" \
  "${API}/contests/${CONTEST_ID}/problems")
ATTACH_PROBLEM_ID=$(echo "${ATTACH}" | jq -r '.problem_id // empty' 2>/dev/null || true)
[[ -n "${ATTACH_PROBLEM_ID}" ]] || { echo "attach to contest failed: ${ATTACH}" >&2; exit 1; }
echo "  attached to contest: $(echo "${ATTACH}" | jq -c .)"

# --- ship state to loadgen ---
KEY=~/.ssh/id_ed25519
ssh -i ${KEY} -o StrictHostKeyChecking=no root@10.104.0.16 'mkdir -p /opt/broccoli/stress-test'
cat <<EOF | ssh -i ${KEY} -o StrictHostKeyChecking=no root@10.104.0.16 'cat > /opt/broccoli/stress-test/run-state.env'
ADMIN_TOKEN=${TOKEN}
CONTEST_ID=${CONTEST_ID}
PROBLEM_ID=${PROBLEM_ID}
CONTEST_TYPE=${CONTEST_TYPE}
PROBLEM_TYPE=${PROBLEM_TYPE}
RUN_ID=signpost-2026-05-12
EOF

echo "seed-done"
REMOTE

log "Signpost contest seeded. State written to broccoli-loadgen:/opt/broccoli/stress-test/run-state.env"
