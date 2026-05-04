#!/usr/bin/env bash
# Builds every plugin in `plugins/*` that declares a `[server]` entry in
# `plugin.toml` and copies the produced WebAssembly module to the path the
# manifest expects. Used by CI before running tests, and by developers who
# want to refresh local plugin artefacts in one shot.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PLUGINS_DIR="${ROOT_DIR}/plugins"

if [[ ! -d "${PLUGINS_DIR}" ]]; then
  echo "no plugins directory at ${PLUGINS_DIR}" >&2
  exit 1
fi

shopt -s nullglob
built_any=0

for plugin_dir in "${PLUGINS_DIR}"/*/; do
  plugin_name=$(basename "${plugin_dir}")
  manifest="${plugin_dir}plugin.toml"
  cargo_toml="${plugin_dir}Cargo.toml"

  if [[ ! -f "${manifest}" || ! -f "${cargo_toml}" ]]; then
    continue
  fi

  entry=$(awk '
    /^\[server\][[:space:]]*$/ { in_server = 1; next }
    /^\[/                       { in_server = 0 }
    in_server && /^entry[[:space:]]*=/ {
      sub(/^entry[[:space:]]*=[[:space:]]*/, "")
      gsub(/^["'\'']|["'\'']$/, "")
      print
      exit
    }
  ' "${manifest}")

  if [[ -z "${entry}" ]]; then
    continue
  fi

  echo "==> Building plugin: ${plugin_name} -> ${entry}"
  (cd "${plugin_dir}" && cargo build --target wasm32-wasip1 --release --locked)

  built=( "${plugin_dir}target/wasm32-wasip1/release/"*.wasm )
  if [[ ${#built[@]} -eq 0 ]]; then
    echo "error: no .wasm produced for ${plugin_name}" >&2
    exit 1
  fi
  if [[ ${#built[@]} -gt 1 ]]; then
    echo "error: multiple .wasm produced for ${plugin_name}: ${built[*]}" >&2
    exit 1
  fi

  cp "${built[0]}" "${plugin_dir}${entry}"
  built_any=1
done

if [[ ${built_any} -eq 0 ]]; then
  echo "warning: no plugins with [server] entries were built" >&2
fi
