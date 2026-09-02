#!/usr/bin/env bash
# Resolve a config value by precedence: CLI flag > env > interactive prompt >
# default. The single seam that makes setup.sh both interactive and scriptable.
# Interactive iff stdin is a tty AND --non-interactive was not forced.
set -euo pipefail

_answers_interactive() {
  [ -n "${FLAG_NON_INTERACTIVE:-}" ] && return 1
  [ -t 0 ]
}

_answers_flagname() { echo "--$(printf '%s' "$1" | tr 'A-Z_' 'a-z-')"; }

# answer KEY PROMPT DEFAULT [REQUIRED]
answer() {
  local key="$1" prompt="$2" default="${3:-}" required="${4:-0}"
  local flagvar="FLAG_${key}" envvar="BROCCOLI_SETUP_${key}" v reply
  v="${!flagvar:-}"; [ -n "$v" ] || v="${!envvar:-}"
  if [ -n "$v" ]; then echo "$v"; return 0; fi
  if _answers_interactive; then
    if [ -n "$default" ]; then
      read -r -p "$prompt [$default]: " reply || true
      echo "${reply:-$default}"
    else
      read -r -p "$prompt: " reply || true
      echo "$reply"
    fi
    return 0
  fi
  if [ -n "$default" ]; then echo "$default"; return 0; fi
  if [ "$required" = "1" ]; then
    echo "ERROR: required value '$key' unset; pass $(_answers_flagname "$key") or \$$envvar" >&2
    exit 2
  fi
  echo ""
}

# answer_secret KEY PROMPT  (silent interactive read; required, no default)
answer_secret() {
  local key="$1" prompt="$2"
  local flagvar="FLAG_${key}" envvar="BROCCOLI_SETUP_${key}" v reply
  v="${!flagvar:-}"; [ -n "$v" ] || v="${!envvar:-}"
  if [ -n "$v" ]; then echo "$v"; return 0; fi
  if _answers_interactive; then
    read -rs -p "$prompt: " reply || true; echo >&2
    echo "$reply"; return 0
  fi
  echo "ERROR: required secret '$key' unset; pass $(_answers_flagname "$key") or \$$envvar" >&2
  exit 2
}
