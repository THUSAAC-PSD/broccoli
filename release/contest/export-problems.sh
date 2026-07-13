#!/usr/bin/env bash
# Export all (non-deleted) problems from a running Broccoli server into a
# portable, self-contained archive that import-problems.sh can restore onto a
# fresh contest server. Dumps the problem-defining Postgres tables plus the
# exact object-storage blobs they reference. No internet / no aws-cli needed
# (S3 copy is pure stdlib python via ./s3copy.py) -- safe for an airgapped LAN.
#
# What is exported (problem definitions only):
#   problem, test_case, problem_attachment, additional_file  (non-deleted only)
#   plugin_config  (global 'plugin' scope + per-problem 'problem' scope)
#   referenced object-storage blobs (large test-case bodies, attachments,
#                                    communication manager sources)
# What is NOT exported (re-created on the contest server by the admin):
#   contests, users/roles, submissions/results, the plugin registry (the server
#   re-registers plugins from plugins_dir on boot), transient plugin_storage.
#
# Usage:
#   ./export-problems.sh [--config /data/broccoli/config/config.toml]
#                        [--out broccoli-problems-<ts>.tar.gz]
#                        [--contest N]     # only problems in contest N
#                        [--problems 1,2,3]# only these problem ids (overrides --contest)
#                        [--all-blobs]     # ship the whole bucket, not just refs
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CONFIG="/data/broccoli/config/config.toml"
OUT=""
ALL_BLOBS=0
CONTEST=""    # --contest N: export only problems in contest N (else all non-deleted)
PROBLEMS=""   # --problems 1,2,3: export only these ids (overrides --contest)
while [ $# -gt 0 ]; do
  case "$1" in
    --config) CONFIG="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --all-blobs) ALL_BLOBS=1; shift ;;
    --contest) CONTEST="$2"; shift 2 ;;
    --problems) PROBLEMS="$2"; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
[ -f "$CONFIG" ] || { echo "config not found: $CONFIG" >&2; exit 2; }
command -v pg_dump >/dev/null || { echo "pg_dump not found" >&2; exit 2; }
command -v psql >/dev/null    || { echo "psql not found" >&2; exit 2; }
TS="$(date -u +%Y%m%d-%H%M%S)"
[ -n "$OUT" ] || OUT="broccoli-problems-${TS}.tar.gz"

# --- read DB url + S3 creds from the broccoli config.toml ---
dburl="$(awk '/^\[database\]/{f=1;next} /^\[/{f=0} f' "$CONFIG" | grep -E '^[[:space:]]*url' | head -1 | sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//')"
s3sec="$(awk '/^\[storage.object_storage\]/{f=1;next} /^\[/{f=0} f' "$CONFIG")"
s3val(){ printf '%s\n' "$s3sec" | grep -E "^[[:space:]]*$1[[:space:]]*=" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//'; }
export S3_ENDPOINT="$(s3val endpoint)" S3_BUCKET="$(s3val bucket)" S3_REGION="$(s3val region)"
export S3_ACCESS_KEY="$(s3val access_key)" S3_SECRET_KEY="$(s3val secret_key)"
[ -n "$dburl" ] || { echo "could not read [database].url from $CONFIG" >&2; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/db" "$WORK/blobs"
PSQL=(psql "$dburl" -v ON_ERROR_STOP=1 -At)

echo ">> exporting from $dburl"
SRCVER="$("${PSQL[@]}" -c 'show server_version' | awk '{print $1}')"

# Which problems to export (always non-deleted). Default: all. --problems is an
# explicit id list; --contest restricts to a contest's problems. Both inputs are
# sanitized to digits/commas only, so they are safe to interpolate.
PROBSEL="SELECT id FROM problem WHERE deleted_at IS NULL"
if [ -n "$PROBLEMS" ]; then
  ids="$(printf '%s' "$PROBLEMS" | tr -cd '0-9,')"
  [ -n "$ids" ] || { echo "--problems needs a comma-separated id list" >&2; exit 2; }
  PROBSEL="$PROBSEL AND id IN ($ids)"
elif [ -n "$CONTEST" ]; then
  cid="$(printf '%s' "$CONTEST" | tr -cd '0-9')"
  [ -n "$cid" ] || { echo "--contest needs a numeric contest id" >&2; exit 2; }
  PROBSEL="$PROBSEL AND id IN (SELECT problem_id FROM contest_problem WHERE contest_id=$cid)"
fi
# active problem-id predicate for child tables, reused below
ACTIVE="problem_id IN ($PROBSEL)"

# --- per-table COPY (native text format: round-trips newlines/jsonb exactly) ---
copy_out(){ # $1 table  $2 select-sql
  "${PSQL[@]}" -c "\copy ($2) TO '$WORK/db/$1.dat'"
  local n; n="$("${PSQL[@]}" -c "SELECT count(*) FROM ($2) s")"
  echo "   $1: $n rows"
  echo "$n" > "$WORK/db/$1.count"
}
copy_out problem            "SELECT * FROM problem WHERE id IN ($PROBSEL)"
copy_out test_case          "SELECT * FROM test_case WHERE $ACTIVE"
copy_out problem_attachment "SELECT * FROM problem_attachment WHERE $ACTIVE"
copy_out additional_file    "SELECT * FROM additional_file WHERE $ACTIVE"
copy_out plugin_config      "SELECT * FROM plugin_config WHERE scope='plugin' OR (scope='problem' AND ref_id IN (SELECT id::text FROM ($PROBSEL) s))"

# --- collect referenced blob hashes -> keyfile (shard XX/rest) ---
"${PSQL[@]}" -c "
WITH active AS ($PROBSEL),
hashes AS (
  SELECT input_blob_hash h FROM test_case WHERE problem_id IN (SELECT id FROM active) AND input_blob_hash IS NOT NULL
  UNION SELECT expected_output_blob_hash FROM test_case WHERE problem_id IN (SELECT id FROM active) AND expected_output_blob_hash IS NOT NULL
  UNION SELECT content_hash FROM problem_attachment WHERE problem_id IN (SELECT id FROM active)
  UNION SELECT content_hash FROM additional_file WHERE problem_id IN (SELECT id FROM active)
  UNION SELECT (regexp_matches(config::text,'[0-9a-f]{64}','g'))[1]
        FROM plugin_config
        WHERE scope='plugin' OR (scope='problem' AND ref_id IN (SELECT id::text FROM active))
)
SELECT substr(h,1,2)||'/'||substr(h,3) FROM hashes WHERE h ~ '^[0-9a-f]{64}\$'
" | sort -u > "$WORK/blob_keys.txt"
NREF="$(wc -l < "$WORK/blob_keys.txt" | tr -d ' ')"
echo ">> $NREF referenced blob(s)"

# --- dump blobs from object storage (stdlib s3copy; no aws-cli) ---
if [ "$ALL_BLOBS" = 1 ]; then
  NBLOB="$(python3 "$HERE/s3copy.py" dump --dir "$WORK/blobs")"
else
  if [ "$NREF" -gt 0 ]; then
    NBLOB="$(python3 "$HERE/s3copy.py" dump --dir "$WORK/blobs" --keys-file "$WORK/blob_keys.txt")"
  else
    NBLOB=0
  fi
fi
echo ">> $NBLOB blob(s) dumped"

# --- manifest ---
NPROB="$(cat "$WORK/db/problem.count")"
cat > "$WORK/manifest.json" <<JSON
{
  "format": "broccoli-problems/v1",
  "created_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source_host": "$(hostname)",
  "source_pg_version": "$SRCVER",
  "tables": ["problem","test_case","problem_attachment","additional_file","plugin_config"],
  "copy_format": "postgres-text",
  "problems": $NPROB,
  "referenced_blobs": $NREF,
  "blobs_dumped": $NBLOB,
  "all_blobs": $([ "$ALL_BLOBS" = 1 ] && echo true || echo false)
}
JSON

tar -czf "$OUT" -C "$WORK" .
echo ">> wrote $OUT ($(du -h "$OUT" | cut -f1)): $NPROB problems, $NBLOB blobs"
