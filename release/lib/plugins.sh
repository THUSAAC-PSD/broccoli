# shellcheck shell=bash
# How release bundles ship plugins. Sourced by every bundle builder, by the
# server image build, and by every installer, so they all agree.
#
# A plugin says how it ships in its own plugin.toml:
#
#   [bundle]
#   mode = "default"    # shipped in plugins/ and loaded (also when absent)
#   mode = "optional"   # shipped in plugins-available/, loaded once enabled
#   mode = "none"       # never shipped (test fixtures)
#
# The host ignores this section; only packaging reads it.

# plugin_bundle_mode DIR: print the [bundle] mode of the plugin in DIR.
plugin_bundle_mode() {
  local mode
  mode="$(awk '
    /^[[:space:]]*\[/ { in_bundle = ($0 ~ /^[[:space:]]*\[bundle\][[:space:]]*$/); next }
    in_bundle && /^[[:space:]]*mode[[:space:]]*=/ {
      sub(/^[^=]*=[[:space:]]*"/, ""); sub(/".*/, ""); print; exit
    }' "$1/plugin.toml")"
  echo "${mode:-default}"
}

# copy_plugin SRC_PARENT NAME DEST_PARENT: copy one plugin, without build
# caches and dependencies.
copy_plugin() {
  mkdir -p "$3"
  tar -C "$1" \
    --exclude='.git' --exclude='.DS_Store' --exclude='target' \
    --exclude='node_modules' --exclude='.cache' --exclude='build' \
    -cf - "$2" | tar -C "$3" -xf -
}

# stage_bundle_plugins SRC DEST: copy every plugin under SRC into
# DEST/plugins or DEST/plugins-available according to its mode.
stage_bundle_plugins() {
  local src="$1" dest="$2" dir name mode
  mkdir -p "$dest/plugins" "$dest/plugins-available"
  for dir in "$src"/*/; do
    [ -f "$dir/plugin.toml" ] || continue
    name="$(basename "$dir")"
    mode="$(plugin_bundle_mode "$dir")"
    case "$mode" in
      default) copy_plugin "$src" "$name" "$dest/plugins" ;;
      optional) copy_plugin "$src" "$name" "$dest/plugins-available" ;;
      none) ;;
      *) echo "plugins/$name: unknown [bundle] mode \"$mode\"" >&2; return 1 ;;
    esac
  done
}

# enable_bundle_plugins DIR NAME...: copy optional plugins from
# DIR/plugins-available into DIR/plugins. Enabling one that is already
# enabled replaces it with the bundled copy.
enable_bundle_plugins() {
  local dir="$1" name
  shift
  for name in "$@"; do
    if [ ! -f "$dir/plugins-available/$name/plugin.toml" ]; then
      echo "no optional plugin \"$name\" in $dir/plugins-available" >&2
      echo "available: $(list_optional_plugins "$dir" | tr '\n' ' ')" >&2
      return 1
    fi
    rm -rf "${dir:?}/plugins/$name"
    copy_plugin "$dir/plugins-available" "$name" "$dir/plugins"
    echo "enabled plugin $name"
  done
}

# list_optional_plugins DIR: names of the optional plugins shipped in DIR.
list_optional_plugins() {
  local d
  for d in "$1"/plugins-available/*/; do
    [ -f "$d/plugin.toml" ] && basename "$d"
  done
  return 0
}

# enable_requested_plugins DIR: enable the plugins named in BROCCOLI_PLUGINS
# (space or comma separated). Installers call this before starting services.
enable_requested_plugins() {
  local names
  names="$(printf '%s' "${BROCCOLI_PLUGINS:-}" | tr ',' ' ')"
  [ -n "${names// /}" ] || return 0
  # shellcheck disable=SC2086
  enable_bundle_plugins "$1" $names
}
