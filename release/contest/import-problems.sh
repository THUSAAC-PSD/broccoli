#!/usr/bin/env bash
# Import a broccoli-problems archive (made by export-problems.sh) into a FRESH
# contest server: restores the problem tables and pushes the referenced blobs
# into the contest object store. No internet / no aws-cli needed.
#
# Prerequisites on the target:
#   - the Broccoli server has started at least once (it auto-creates the schema
#     and registers its plugins on boot)
#   - the target's config.toml points at the contest Postgres + SeaweedFS
#
# Usage:
#   ./import-problems.sh ARCHIVE.tar.gz [--config /data/broccoli/config/config.toml]
#                                       [--truncate] [--wipe-dependents] [--yes]
#   --truncate        : wipe existing problem rows on the target first (needed if
#                       the target already has problems; default refuses a
#                       non-empty target). Refuses if submissions, code runs or
#                       contest-problem links exist on the target.
#   --wipe-dependents : with --truncate, ALSO wipe rows that reference the
#                       problems: submission (+judgements/test case results),
#                       code_run (+results) and contest_problem links.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ARCHIVE="${1:-}"; shift || true
[ -n "$ARCHIVE" ] && [ -f "$ARCHIVE" ] || { echo "usage: ./import-problems.sh ARCHIVE.tar.gz [--config C] [--truncate] [--wipe-dependents] [--yes]" >&2; exit 2; }
CONFIG="/data/broccoli/config/config.toml"; TRUNCATE=0; WIPE_DEPENDENTS=0; YES=0
while [ $# -gt 0 ]; do
  case "$1" in
    --config) CONFIG="$2"; shift 2 ;;
    --truncate) TRUNCATE=1; shift ;;
    --wipe-dependents) WIPE_DEPENDENTS=1; shift ;;
    --yes) YES=1; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
[ -f "$CONFIG" ] || { echo "config not found: $CONFIG" >&2; exit 2; }
command -v psql >/dev/null || { echo "psql not found" >&2; exit 2; }

dburl="$(awk '/^\[database\]/{f=1;next} /^\[/{f=0} f' "$CONFIG" | grep -E '^[[:space:]]*url' | head -1 | sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//')"
s3sec="$(awk '/^\[storage.object_storage\]/{f=1;next} /^\[/{f=0} f' "$CONFIG")"
s3val(){ printf '%s\n' "$s3sec" | grep -E "^[[:space:]]*$1[[:space:]]*=" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//'; }
export S3_ENDPOINT="$(s3val endpoint)" S3_BUCKET="$(s3val bucket)" S3_REGION="$(s3val region)"
export S3_ACCESS_KEY="$(s3val access_key)" S3_SECRET_KEY="$(s3val secret_key)"
[ -n "$dburl" ] || { echo "could not read [database].url from $CONFIG" >&2; exit 2; }

WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
tar -xzf "$ARCHIVE" -C "$WORK"
[ -f "$WORK/manifest.json" ] || { echo "not a broccoli-problems archive (no manifest.json)" >&2; exit 2; }
echo ">> manifest:"; sed 's/^/   /' "$WORK/manifest.json"
PSQL=(psql "$dburl" -v ON_ERROR_STOP=1 -At)

# schema must exist (server booted at least once)
have="$("${PSQL[@]}" -c "SELECT count(*) FROM information_schema.tables WHERE table_name IN ('problem','test_case','plugin_config')")"
[ "$have" = "3" ] || { echo "ERROR: target schema missing (problem/test_case/plugin_config). Start the server once first." >&2; exit 2; }

existing="$("${PSQL[@]}" -c "SELECT count(*) FROM problem WHERE deleted_at IS NULL")"
if [ "$existing" -gt 0 ] && [ "$TRUNCATE" = 0 ]; then
  echo "ERROR: target already has $existing problem(s). Re-run with --truncate to replace them." >&2
  exit 2
fi

# Tables that hold FK references onto problem/test_case (Postgres refuses a
# plain TRUNCATE of the problem tables unless they are truncated too, and we
# deliberately avoid CASCADE so nothing gets wiped implicitly).
PROBLEM_TABLES="test_case, problem_attachment, additional_file, problem"
DEPENDENT_TABLES="test_case_result, submission_judgement, submission, code_run_result, code_run, contest_problem"
if [ "$TRUNCATE" = 1 ]; then
  nsub="$("${PSQL[@]}" -c "SELECT count(*) FROM submission")"
  ncr="$("${PSQL[@]}" -c "SELECT count(*) FROM code_run")"
  ncp="$("${PSQL[@]}" -c "SELECT count(*) FROM contest_problem")"
  if [ "$((nsub + ncr + ncp))" -gt 0 ] && [ "$WIPE_DEPENDENTS" = 0 ]; then
    {
      echo "ERROR: rows on the target reference the problems being replaced:"
      echo "         submission=$nsub code_run=$ncr contest_problem=$ncp"
      echo "       Replacing the problems would destroy them (plus their judgements"
      echo "       and per-test-case results). Refusing."
      echo "       Re-run with --truncate --wipe-dependents to wipe them too."
    } >&2
    exit 2
  fi
  if [ "$YES" = 0 ]; then
    echo "About to TRUNCATE these tables on the target and replace plugin_config:"
    echo "  $PROBLEM_TABLES"
    echo "  $DEPENDENT_TABLES"
    echo "  (currently: submissions=$nsub, code runs=$ncr, contest-problem links=$ncp)"
    printf "Type 'yes' to continue: "
    read -r ans; [ "$ans" = "yes" ] || { echo "aborted"; exit 1; }
  fi
fi

# build restore SQL (client-side \copy paths are relative to $WORK)
SQL="$WORK/restore.sql"
{
  echo "BEGIN;"
  if [ "$TRUNCATE" = 1 ]; then
    # No CASCADE: every affected table is named explicitly (dependents were
    # verified empty above unless --wipe-dependents was given), so an
    # unexpected FK from any other table aborts the transaction instead of
    # silently truncating it.
    echo "TRUNCATE $DEPENDENT_TABLES, $PROBLEM_TABLES RESTART IDENTITY;"
  fi
  # FK-safe order: parent problem first, then children
  echo "\\copy problem FROM 'db/problem.dat'"
  echo "\\copy test_case FROM 'db/test_case.dat'"
  echo "\\copy problem_attachment FROM 'db/problem_attachment.dat'"
  echo "\\copy additional_file FROM 'db/additional_file.dat'"
  # plugin_config: upsert via temp table (target may already hold default globals)
  echo "CREATE TEMP TABLE _pc (LIKE plugin_config INCLUDING DEFAULTS);"
  echo "\\copy _pc FROM 'db/plugin_config.dat'"
  echo "INSERT INTO plugin_config SELECT * FROM _pc ON CONFLICT (scope,ref_id,namespace) DO UPDATE SET config=EXCLUDED.config, enabled=EXCLUDED.enabled, position=EXCLUDED.position, updated_at=EXCLUDED.updated_at;"
  # reset serial sequences so newly-created rows don't collide
  echo "SELECT setval(pg_get_serial_sequence('problem','id'), GREATEST((SELECT COALESCE(max(id),1) FROM problem),1));"
  echo "SELECT setval(pg_get_serial_sequence('test_case','id'), GREATEST((SELECT COALESCE(max(id),1) FROM test_case),1));"
  echo "COMMIT;"
} > "$SQL"

echo ">> restoring database..."
( cd "$WORK" && psql "$dburl" -v ON_ERROR_STOP=1 -q -f "$SQL" )

echo ">> pushing blobs into object storage..."
NLOADED=0
if [ -d "$WORK/blobs" ] && [ -n "$(ls -A "$WORK/blobs" 2>/dev/null)" ]; then
  NLOADED="$(python3 "$HERE/s3copy.py" load --dir "$WORK/blobs")"
fi

# verify
GOTP="$("${PSQL[@]}" -c "SELECT count(*) FROM problem WHERE deleted_at IS NULL")"
GOTT="$("${PSQL[@]}" -c "SELECT count(*) FROM test_case")"
WANTP="$(grep -o '"problems": *[0-9]*' "$WORK/manifest.json" | grep -o '[0-9]*')"
echo ">> done: problems=$GOTP (manifest $WANTP), test_cases=$GOTT, blobs_loaded=$NLOADED"
[ "$GOTP" = "$WANTP" ] || { echo "WARN: problem count mismatch (target had pre-existing rows?)" >&2; }
