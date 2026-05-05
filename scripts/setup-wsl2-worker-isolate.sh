#!/usr/bin/env bash
set -euo pipefail

ISOLATE_VERSION="${ISOLATE_VERSION:-v2.0}"
ISOLATE_REPO="${ISOLATE_REPO:-https://github.com/ioi/isolate.git}"
CGROUP_ROOT="${CGROUP_ROOT:-/sys/fs/cgroup}"
ISOLATE_PARENT="${ISOLATE_PARENT:-isolate}"
ISOLATE_RUN_DIR="${ISOLATE_RUN_DIR:-/run/isolate}"
ISOLATE_BOX_DIR="${ISOLATE_BOX_DIR:-/var/local/lib/isolate}"
REQUIRED_CONTROLLERS="${REQUIRED_CONTROLLERS:-memory cpu pids}"
INSTALL_TOOLCHAINS="${INSTALL_TOOLCHAINS:-true}"
SKIP_APT=false
ALLOW_NON_WSL=false
VERIFY_ONLY=false
ORIGINAL_ARGS=("$@")

usage() {
  cat >&2 <<'EOF'
usage: scripts/setup-wsl2-worker-isolate.sh [options]

Prepares a WSL2 distro to run Broccoli workers without Docker by installing
ioi/isolate and creating the cgroup v2 subtree expected by isolate --cg.

Options:
  --skip-apt          Do not install apt packages.
  --no-toolchains     Do not install gcc/g++/python3/default-jdk-headless.
  --verify-only       Only verify isolate and cgroup setup.
  --allow-non-wsl     Allow running on non-WSL Linux hosts.
  -h, --help          Show this help.

Environment:
  ISOLATE_VERSION     isolate tag to build, default v2.0.
  ISOLATE_REPO        isolate git repo, default https://github.com/ioi/isolate.git.
  REQUIRED_CONTROLLERS cgroup controllers to enable, default "memory cpu pids".
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

log() {
  echo "==> $*"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-apt) SKIP_APT=true ;;
    --no-toolchains) INSTALL_TOOLCHAINS=false ;;
    --verify-only) VERIFY_ONLY=true ;;
    --allow-non-wsl) ALLOW_NON_WSL=true ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

if [[ "$(uname -s)" != "Linux" ]]; then
  die "this script must run inside the target WSL2/Linux distro"
fi

if [[ "$ALLOW_NON_WSL" != "true" ]]; then
  if [[ -z "${WSL_DISTRO_NAME:-}" ]] &&
    ! grep -qiE 'microsoft|wsl' /proc/version /proc/sys/kernel/osrelease 2>/dev/null; then
    die "this does not look like WSL2; pass --allow-non-wsl to override"
  fi
fi

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
  if ! command -v sudo >/dev/null 2>&1; then
    die "root privileges are required and sudo is not installed"
  fi
  exec sudo -E bash "$0" "${ORIGINAL_ARGS[@]}"
fi

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required"
}

have_controller() {
  local controller="$1"
  grep -qw "$controller" "$CGROUP_ROOT/cgroup.controllers" 2>/dev/null
}

enable_controller() {
  local controller="$1"
  local target="$2"
  printf '+%s\n' "$controller" > "$target/cgroup.subtree_control"
}

move_processes() {
  local from="$1"
  local to="$2"

  [[ -f "$from/cgroup.procs" ]] || return 0
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    printf '%s\n' "$pid" > "$to/cgroup.procs" 2>/dev/null || true
  done < "$from/cgroup.procs"
}

install_packages() {
  [[ "$SKIP_APT" == "false" ]] || return 0
  need_cmd apt-get

  local packages=(
    build-essential
    ca-certificates
    git
    libcap-dev
    libseccomp-dev
    libsystemd-dev
    make
    pkg-config
  )
  if [[ "$INSTALL_TOOLCHAINS" == "true" ]]; then
    packages+=(
      default-jdk-headless
      g++
      gcc
      python3
    )
  fi

  log "installing apt packages"
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${packages[@]}"
}

install_isolate() {
  if command -v isolate >/dev/null 2>&1 && isolate --version >/dev/null 2>&1; then
    log "isolate already installed at $(command -v isolate)"
    return 0
  fi

  need_cmd git
  need_cmd make

  local work
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT

  log "building isolate ${ISOLATE_VERSION}"
  git clone --branch "$ISOLATE_VERSION" --depth 1 "$ISOLATE_REPO" "$work/isolate"
  make -C "$work/isolate" isolate isolate-cg-keeper

  log "installing isolate"
  install -m 4755 "$work/isolate/isolate" /usr/local/bin/isolate
  install -m 0755 "$work/isolate/isolate-check-environment" /usr/local/bin/isolate-check-environment
  install -m 0755 "$work/isolate/isolate-cg-keeper" /usr/local/sbin/isolate-cg-keeper
  install -d -m 0755 /usr/local/etc /usr/local/share/isolate
  install -m 0644 "$work/isolate/default.cf" /usr/local/etc/isolate
  install -m 0644 "$work/isolate/default.cf" /usr/local/share/isolate/default.cf
}

prepare_runtime_dirs() {
  log "preparing isolate runtime directories"
  install -d -m 0755 "$ISOLATE_RUN_DIR"
  install -d -m 0755 "$ISOLATE_BOX_DIR"
  install -d -m 0755 ./data
}

setup_cgroup_tree() {
  [[ -f "$CGROUP_ROOT/cgroup.controllers" ]] ||
    die "cgroup v2 is not mounted at $CGROUP_ROOT"

  local controller
  for controller in $REQUIRED_CONTROLLERS; do
    have_controller "$controller" || die "cgroup controller '$controller' is not available"
  done

  local base="$1"
  local parent="$base/$ISOLATE_PARENT"
  local holder="$base/broccoli-worker-host"

  log "creating isolate cgroup tree under $parent"
  mkdir -p "$parent" "$holder"

  # cgroup v2 domain controllers cannot be enabled on a cgroup that has direct
  # processes. Move any current processes into a sibling holder first.
  move_processes "$base" "$holder"

  for controller in $REQUIRED_CONTROLLERS; do
    enable_controller "$controller" "$base" ||
      die "could not enable cgroup controller '$controller' on $base"
  done

  move_processes "$base" "$holder"

  for controller in $REQUIRED_CONTROLLERS; do
    enable_controller "$controller" "$parent" ||
      die "could not enable cgroup controller '$controller' on $parent"
  done

  printf '%s\n' "$parent" > "$ISOLATE_RUN_DIR/cgroup"
  chmod 0755 "$ISOLATE_RUN_DIR"
  log "wrote $ISOLATE_RUN_DIR/cgroup -> $parent"
}

install_systemd_cgroup_unit() {
  local helper=/usr/local/sbin/broccoli-worker-isolate-cgroups
  local unit=/etc/systemd/system/broccoli-worker-isolate-cgroups.service

  log "installing systemd cgroup setup helper"
  cat > "$helper" <<'HELPER'
#!/usr/bin/env bash
set -euo pipefail

CGROUP_ROOT="${CGROUP_ROOT:-/sys/fs/cgroup}"
ISOLATE_PARENT="${ISOLATE_PARENT:-isolate}"
ISOLATE_RUN_DIR="${ISOLATE_RUN_DIR:-/run/isolate}"
ISOLATE_BOX_DIR="${ISOLATE_BOX_DIR:-/var/local/lib/isolate}"
REQUIRED_CONTROLLERS="${REQUIRED_CONTROLLERS:-memory cpu pids}"

die() {
  echo "error: $*" >&2
  exit 1
}

have_controller() {
  local controller="$1"
  grep -qw "$controller" "$CGROUP_ROOT/cgroup.controllers" 2>/dev/null
}

enable_controller() {
  local controller="$1"
  local target="$2"
  printf '+%s\n' "$controller" > "$target/cgroup.subtree_control"
}

move_processes() {
  local from="$1"
  local to="$2"
  [[ -f "$from/cgroup.procs" ]] || return 0
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    printf '%s\n' "$pid" > "$to/cgroup.procs" 2>/dev/null || true
  done < "$from/cgroup.procs"
}

[[ -f "$CGROUP_ROOT/cgroup.controllers" ]] || die "cgroup v2 is not mounted at $CGROUP_ROOT"

for controller in $REQUIRED_CONTROLLERS; do
  have_controller "$controller" || die "cgroup controller '$controller' is not available"
done

base="$CGROUP_ROOT"
parent="$base/$ISOLATE_PARENT"
holder="$base/broccoli-worker-host"

install -d -m 0755 "$ISOLATE_RUN_DIR" "$ISOLATE_BOX_DIR"
mkdir -p "$parent" "$holder"
move_processes "$base" "$holder"

for controller in $REQUIRED_CONTROLLERS; do
  enable_controller "$controller" "$base" ||
    die "could not enable cgroup controller '$controller' on $base"
done

move_processes "$base" "$holder"

for controller in $REQUIRED_CONTROLLERS; do
  enable_controller "$controller" "$parent" ||
    die "could not enable cgroup controller '$controller' on $parent"
done

printf '%s\n' "$parent" > "$ISOLATE_RUN_DIR/cgroup"
chmod 0755 "$ISOLATE_RUN_DIR"
echo "isolate cgroup v2 setup complete: $parent"
HELPER
  chmod 0755 "$helper"

  cat > "$unit" <<'UNIT'
[Unit]
Description=Prepare cgroup v2 subtree for Broccoli isolate workers
Documentation=https://github.com/THUSAAC-PSD/broccoli

[Service]
Type=oneshot
RemainAfterExit=yes
Delegate=yes
ExecStart=/usr/local/sbin/broccoli-worker-isolate-cgroups

[Install]
WantedBy=multi-user.target
UNIT

  systemctl daemon-reload
  systemctl enable --now broccoli-worker-isolate-cgroups.service
}

setup_cgroups() {
  # The isolate parent cgroup must outlive the setup command. Do not create it
  # under the setup process's systemd service cgroup, because systemd removes
  # oneshot service cgroups after the service exits.
  if [[ "$ALLOW_NON_WSL" == "true" ]]; then
    log "configuring cgroups directly under $CGROUP_ROOT"
    setup_cgroup_tree "$CGROUP_ROOT"
    return 0
  fi

  if command -v systemctl >/dev/null 2>&1 &&
    [[ "$(ps -p 1 -o comm= 2>/dev/null || true)" == "systemd" ]]; then
    install_systemd_cgroup_unit
  else
    log "systemd is not pid 1; configuring cgroups directly for this WSL session"
    setup_cgroup_tree "$CGROUP_ROOT"
  fi
}

verify_setup() {
  need_cmd isolate
  [[ -u "$(command -v isolate)" ]] || die "isolate is missing the setuid bit"
  install -d -m 0755 "$ISOLATE_RUN_DIR" "$ISOLATE_BOX_DIR"
  [[ -f "$ISOLATE_RUN_DIR/cgroup" ]] || die "$ISOLATE_RUN_DIR/cgroup is missing"

  local parent
  parent="$(cat "$ISOLATE_RUN_DIR/cgroup")"
  [[ -d "$parent" ]] || die "configured isolate cgroup does not exist: $parent"

  log "running isolate cgroup smoke test"
  local box_id=998
  isolate --cg --box-id="$box_id" --cleanup >/dev/null 2>&1 || true
  isolate --cg --box-id="$box_id" --init >/dev/null
  isolate --cg --box-id="$box_id" --run -- /bin/true >/dev/null
  isolate --cg --box-id="$box_id" --cleanup >/dev/null

  log "isolate is ready for Broccoli workers"
}

if [[ "$VERIFY_ONLY" == "false" ]]; then
  install_packages
  install_isolate
  prepare_runtime_dirs
  setup_cgroups
fi

verify_setup

cat <<'EOF'

Example worker launch:

  BROCCOLI__WORKER__ID=wsl-worker-1 \
  BROCCOLI__WORKER__SANDBOX_BACKEND=isolate \
  BROCCOLI__WORKER__ISOLATE_BIN=/usr/local/bin/isolate \
  BROCCOLI__WORKER__ENABLE_CGROUPS=true \
  BROCCOLI__DATABASE__URL='postgres://USER:PASSWORD@HOST:5432/broccoli' \
  BROCCOLI__MQ__URL='redis://[:PASSWORD@]HOST:6379' \
  cargo run -p worker

Use a unique BROCCOLI__WORKER__ID for each worker instance.
EOF
