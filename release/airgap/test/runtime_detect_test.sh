#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../lib/runtime.sh"

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
mkbin() { # name info_rc composeflag composerc
  local n="$1" info_rc="$2" cflag="$3" crc="$4"
  cat > "$tmp/bin/$n" <<EOF
#!/usr/bin/env bash
case "\$1" in
  info) exit $info_rc ;;
  $cflag) [ "\$2" = version ] && exit $crc ; exit 1 ;;
  version) exit $crc ;;
  *) exit 1 ;;
esac
EOF
  chmod +x "$tmp/bin/$n"
}

# scenario A: only podman works -> engine=podman, compose="podman compose"
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
mkbin docker 1 compose 1
mkbin podman 0 compose 0
PATH="$tmp/bin:$PATH" BROCCOLI_ENGINE="" bash -c '. '"$here"'/../lib/runtime.sh; e=$(runtime_engine); [ "$e" = podman ] || { echo "FAIL A engine=$e"; exit 1; }; c=$(runtime_compose "$e"); [ "$c" = "podman compose" ] || { echo "FAIL A compose=$c"; exit 1; }'

# scenario B: both work -> docker preferred
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
mkbin docker 0 compose 0
mkbin podman 0 compose 0
PATH="$tmp/bin:$PATH" BROCCOLI_ENGINE="" bash -c '. '"$here"'/../lib/runtime.sh; [ "$(runtime_engine)" = docker ] || { echo FAIL B; exit 1; }'

# scenario C: neither works -> ""
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
mkbin docker 1 compose 1
mkbin podman 1 compose 1
PATH="$tmp/bin:$PATH" BROCCOLI_ENGINE="" bash -c '. '"$here"'/../lib/runtime.sh; [ -z "$(runtime_engine)" ] || { echo FAIL C; exit 1; }'

# scenario D: override BROCCOLI_ENGINE=podman while docker also works
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
mkbin docker 0 compose 0
mkbin podman 0 compose 0
PATH="$tmp/bin:$PATH" BROCCOLI_ENGINE=podman bash -c '. '"$here"'/../lib/runtime.sh; [ "$(runtime_engine)" = podman ] || { echo FAIL D; exit 1; }'

# scenario E: podman without `podman compose` falls back to podman-compose
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
mkbin podman 0 xxx 1            # `podman compose version` fails
mkbin podman-compose 0 xxx 0
PATH="$tmp/bin:$PATH" bash -c '. '"$here"'/../lib/runtime.sh; [ "$(runtime_compose podman)" = "podman-compose" ] || { echo FAIL E; exit 1; }'

# scenario F: relabel is a no-op for docker (no chcon needed)
rm -rf "$tmp/bin"; mkdir -p "$tmp/bin"
cat > "$tmp/bin/chcon" <<'E'
#!/usr/bin/env bash
echo "chcon $*" >> "$CHCON_LOG"
E
chmod +x "$tmp/bin/chcon"
cat > "$tmp/bin/getenforce" <<'E'
#!/usr/bin/env bash
echo Enforcing
E
chmod +x "$tmp/bin/getenforce"
export CHCON_LOG="$tmp/chcon.log"; : > "$CHCON_LOG"
mkdir -p "$tmp/target"
PATH="$tmp/bin:$PATH" bash -c '. '"$here"'/../lib/runtime.sh; runtime_relabel docker "'"$tmp"'/target"'
[ -s "$CHCON_LOG" ] && { echo "FAIL F: chcon ran for docker"; exit 1; } || true
# scenario G: relabel runs chcon for podman + enforcing
PATH="$tmp/bin:$PATH" bash -c '. '"$here"'/../lib/runtime.sh; runtime_relabel podman "'"$tmp"'/target"'
grep -q "container_file_t" "$CHCON_LOG" || { echo "FAIL G: chcon did not run for podman+enforcing"; exit 1; }

echo "PASS: runtime detect/compose/relabel"
