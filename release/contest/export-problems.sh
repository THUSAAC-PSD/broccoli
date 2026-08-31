#!/usr/bin/env bash
# Export problems -- or a whole contest -- from a running Broccoli server into
# a portable, self-contained archive that import-problems.sh can restore onto
# a fresh contest server. Dumps the problem-defining Postgres tables plus the
# exact object-storage blobs they reference. No internet / no aws-cli needed
# (S3 copy is pure stdlib python via ./s3copy.py) -- safe for an airgapped LAN.
#
# What is always exported (problem definitions, non-deleted only):
#   problem, test_case, problem_attachment, additional_file
#   plugin_config  (global 'plugin' scope + per-problem 'problem' scope)
#   referenced object-storage blobs (large test-case bodies, attachments,
#                                    communication manager sources)
# What is ADDITIONALLY exported when --contest is given (a "contest bundle"):
#   contest, contest_problem, contest_user, user, user_role, role,
#   role_permission -- i.e. the contest row, its roster, and the roster's
#   accounts/roles. User passwords are blanked unless --with-secrets is given.
# What is NEVER exported: submissions/results, the plugin registry (the
#   server re-registers plugins from plugins_dir on boot), transient
#   plugin_storage.
#
# Usage:
#   ./export-problems.sh [--config /data/broccoli/config/config.toml]
#                        [--out broccoli-problems-<ts>.tar.gz]
#                        [--contest N]     # only problems in contest N; also
#                                          # exports the contest+roster+accounts
#                        [--problems 1,2,3]# only these problem ids (overrides --contest)
#                        [--with-secrets]  # include real password hashes (contest bundle only)
#                        [--dry-run]       # print the \copy statements; touch nothing
#                        [--all-blobs]     # ship the whole bucket, not just refs
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CONFIG="/data/broccoli/config/config.toml"
OUT=""
ALL_BLOBS=0
CONTEST=""      # --contest N: export only problems in contest N (else all non-deleted)
PROBLEMS=""     # --problems 1,2,3: export only these ids (overrides --contest)
WITH_SECRETS=0  # --with-secrets: include real password hashes in a contest bundle
DRY_RUN=0       # --dry-run: print the \copy statements; touch no DB/filesystem/process
while [ $# -gt 0 ]; do
  case "$1" in
    --config) CONFIG="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --all-blobs) ALL_BLOBS=1; shift ;;
    --contest) CONTEST="$2"; shift 2 ;;
    --problems) PROBLEMS="$2"; shift 2 ;;
    --with-secrets) WITH_SECRETS=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) sed -n '2,35p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Sanitize --contest once, up front (digits only): it feeds both the
# problem-selection subquery below (PROBSEL) and the contest-scoped
# roster/account export further down, and the latter runs regardless of
# whether --problems was also given.
cid=""
if [ -n "$CONTEST" ]; then
  cid="$(printf '%s' "$CONTEST" | tr -cd '0-9')"
  [ -n "$cid" ] || { echo "--contest needs a numeric contest id" >&2; exit 2; }
fi

# --dry-run touches no config file, no DB, no object storage, no filesystem
# outside stdout: skip the entire connect/prepare preamble below.
if [ "$DRY_RUN" != 1 ]; then
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

  # Redact the password before printing so a captured stdout (tee/CI log) never
  # leaks the DB credential (import-problems.sh likewise never echoes $dburl).
  safe_dburl="$(printf '%s' "$dburl" | sed -E 's#(://[^:/@]+):[^@]*@#\1:****@#')"
  echo ">> exporting from $safe_dburl"
  SRCVER="$("${PSQL[@]}" -c 'show server_version' | awk '{print $1}')"
fi

# Which problems to export (always non-deleted). Default: all. --problems is an
# explicit id list; --contest restricts to a contest's problems. Both inputs are
# sanitized to digits/commas only, so they are safe to interpolate.
PROBSEL="SELECT id FROM problem WHERE deleted_at IS NULL"
if [ -n "$PROBLEMS" ]; then
  ids="$(printf '%s' "$PROBLEMS" | tr -cd '0-9,')"
  [ -n "$ids" ] || { echo "--problems needs a comma-separated id list" >&2; exit 2; }
  PROBSEL="$PROBSEL AND id IN ($ids)"
elif [ -n "$cid" ]; then
  PROBSEL="$PROBSEL AND id IN (SELECT problem_id FROM contest_problem WHERE contest_id=$cid)"
fi
# active problem-id predicate for child tables, reused below
ACTIVE="problem_id IN ($PROBSEL)"

# --- per-table COPY (native text format: round-trips newlines/jsonb exactly) ---
copy_out(){ # $1 table  $2 select-sql
  if [ "$DRY_RUN" = 1 ]; then echo "-- copy $1"; echo "$2"; return; fi
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

# --- contest bundle: also ship the contest row, its roster, and the roster's
#     accounts/roles (only when --contest is given). Passwords are blanked
#     unless --with-secrets is explicitly passed. role/user_role/role_permission
#     key off role NAME (the schema's role.name is the primary key, not a
#     surrogate role_id), so the subqueries below join on that column.
if [ -n "$cid" ]; then
  copy_out contest         "SELECT * FROM contest WHERE id = ${cid}"
  copy_out contest_problem "SELECT * FROM contest_problem WHERE contest_id = ${cid} AND $ACTIVE"
  copy_out contest_user    "SELECT * FROM contest_user WHERE contest_id = ${cid}"

  USERSEL="SELECT user_id FROM contest_user WHERE contest_id = ${cid}"
  if [ "$WITH_SECRETS" = 1 ]; then PWCOL="password"; else PWCOL="'' AS password"; fi
  copy_out "user"          "SELECT id, username, ${PWCOL}, created_at, deleted_at, credentials_changed_at FROM \"user\" WHERE id IN (${USERSEL})"
  copy_out user_role       "SELECT * FROM user_role WHERE user_id IN (${USERSEL})"
  copy_out role            "SELECT * FROM role WHERE name IN (SELECT role FROM user_role WHERE user_id IN (${USERSEL}))"
  copy_out role_permission "SELECT * FROM role_permission WHERE role IN (SELECT role FROM user_role WHERE user_id IN (${USERSEL}))"
fi

if [ -n "$cid" ]; then MANIFEST_TAG="broccoli-contest/v1"; else MANIFEST_TAG="broccoli-problems/v1"; fi

# --dry-run stops here: no blob collection, no S3 access, no manifest file, no
# tar. Everything above was pure string-building plus copy_out's dry-run echo.
if [ "$DRY_RUN" = 1 ]; then
  if [ -n "$cid" ]; then
    echo "manifest kind=contest tag=$MANIFEST_TAG contest=$cid"
  else
    echo "manifest kind=problems tag=$MANIFEST_TAG"
  fi
  exit 0
fi

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
if [ -n "$cid" ]; then
  TABLES_JSON='["problem","test_case","problem_attachment","additional_file","plugin_config","contest","contest_problem","contest_user","user","user_role","role","role_permission"]'
  CONTEST_FIELD=$'\n  "contest": '"$cid,"
else
  TABLES_JSON='["problem","test_case","problem_attachment","additional_file","plugin_config"]'
  CONTEST_FIELD=""
fi
NPROB="$(cat "$WORK/db/problem.count")"
cat > "$WORK/manifest.json" <<JSON
{
  "format": "$MANIFEST_TAG",${CONTEST_FIELD}
  "created_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source_host": "$(hostname)",
  "source_pg_version": "$SRCVER",
  "tables": $TABLES_JSON,
  "copy_format": "postgres-text",
  "problems": $NPROB,
  "referenced_blobs": $NREF,
  "blobs_dumped": $NBLOB,
  "all_blobs": $([ "$ALL_BLOBS" = 1 ] && echo true || echo false)
}
JSON

tar -czf "$OUT" -C "$WORK" .
echo ">> wrote $OUT ($(du -h "$OUT" | cut -f1)): $NPROB problems, $NBLOB blobs"
