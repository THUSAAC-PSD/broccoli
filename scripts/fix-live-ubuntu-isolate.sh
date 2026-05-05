#!/usr/bin/env bash
set -euo pipefail

CGROUP_ROOT=/sys/fs/cgroup
ISOLATE_PARENT="$CGROUP_ROOT/isolate"
ISOLATE_RUN=/run/isolate
ISOLATE_BOX=/var/local/lib/isolate
CONTROLLERS=(memory cpu pids)

die() {
  echo "error: $*" >&2
  exit 1
}

if [[ "$(uname -s)" != "Linux" ]]; then
  die "run this on the Ubuntu live machine"
fi

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
  exec sudo bash "$0" "$@"
fi

command -v isolate >/dev/null 2>&1 || die "isolate is not installed"
[[ -u "$(command -v isolate)" ]] || die "isolate is installed but is missing the setuid bit"
[[ -f "$CGROUP_ROOT/cgroup.controllers" ]] || die "cgroup v2 is not mounted at $CGROUP_ROOT"

install -d -m 0755 "$ISOLATE_RUN" "$ISOLATE_BOX" "$ISOLATE_PARENT"

for controller in "${CONTROLLERS[@]}"; do
  grep -qw "$controller" "$CGROUP_ROOT/cgroup.controllers" ||
    die "cgroup controller '$controller' is not available"
done

for controller in "${CONTROLLERS[@]}"; do
  echo "+$controller" > "$CGROUP_ROOT/cgroup.subtree_control" 2>/dev/null || true
done

for controller in "${CONTROLLERS[@]}"; do
  if ! echo "+$controller" > "$ISOLATE_PARENT/cgroup.subtree_control" 2>/dev/null; then
    die "could not enable cgroup controller '$controller' on $ISOLATE_PARENT"
  fi
done

rm -rf "$ISOLATE_RUN/cgroup"
printf '%s\n' "$ISOLATE_PARENT" > "$ISOLATE_RUN/cgroup"
chmod 0755 "$ISOLATE_RUN"

box_id=998
isolate --cg --box-id="$box_id" --cleanup >/dev/null 2>&1 || true
isolate --cg --box-id="$box_id" --init >/dev/null
isolate --cg --box-id="$box_id" --run -- /bin/true >/dev/null
isolate --cg --box-id="$box_id" --cleanup >/dev/null

echo "ok: isolate cgroup file is $(cat "$ISOLATE_RUN/cgroup")"
