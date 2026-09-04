#!/usr/bin/env bash
# Install the broccoli LAN root CA into the Linux system trust store.
# Idempotent. TARGET-SIDE: no network.
set -euo pipefail
CRT="${1:?usage: linux.sh <root.crt>}"
dest="/usr/local/share/ca-certificates/broccoli-lan-root.crt"
sudo install -m 0644 "$CRT" "$dest"
sudo update-ca-certificates
echo "installed root CA -> $dest"
echo "note: Firefox uses its own cert store; import $CRT there separately if needed."
