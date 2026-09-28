#!/usr/bin/env bash
# An optional plugin enabled with enable-plugin.sh must not break bundle
# integrity, but its enabled copy must still match the hashed original.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=/dev/null
source "$here/../lib/manifest.sh"
# shellcheck source=/dev/null
source "$here/../../lib/plugins.sh"

B="$(mktemp -d)"
trap 'rm -rf "$B"' EXIT
mkdir -p "$B/compose/plugins/icpc" "$B/compose/plugins-available/codelink-bracket/i18n"
printf 'name = "icpc"\n' > "$B/compose/plugins/icpc/plugin.toml"
printf 'name = "codelink-bracket"\n' > "$B/compose/plugins-available/codelink-bracket/plugin.toml"
printf 'x = 1\n' > "$B/compose/plugins-available/codelink-bracket/i18n/en.toml"
manifest_generate "$B"

enable_bundle_plugins "$B/compose" codelink-bracket >/dev/null
[ -f "$B/compose/plugins/codelink-bracket/i18n/en.toml" ] || { echo "FAIL: plugin not enabled"; exit 1; }
manifest_verify "$B" >/dev/null || { echo "FAIL: enabling a bundled plugin broke verify"; exit 1; }

printf 'x = 2\n' > "$B/compose/plugins/codelink-bracket/i18n/en.toml"
if manifest_verify "$B" >/dev/null 2>&1; then
  echo "FAIL: a modified enabled plugin passed verify"; exit 1
fi
enable_bundle_plugins "$B/compose" codelink-bracket >/dev/null
manifest_verify "$B" >/dev/null || { echo "FAIL: re-enabling did not restore the bundled copy"; exit 1; }

printf 'sneaky\n' > "$B/compose/plugins/codelink-bracket/extra.toml"
if manifest_verify "$B" >/dev/null 2>&1; then
  echo "FAIL: a file added to an enabled plugin passed verify"; exit 1
fi
rm "$B/compose/plugins/codelink-bracket/extra.toml"

if enable_bundle_plugins "$B/compose" no-such-plugin >/dev/null 2>&1; then
  echo "FAIL: enabling an unknown plugin succeeded"; exit 1
fi
echo PASS
