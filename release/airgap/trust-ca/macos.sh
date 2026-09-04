#!/usr/bin/env bash
# Install the broccoli LAN root CA into the macOS System keychain.
# Idempotent (re-adding the same cert is a no-op). TARGET-SIDE: no network.
set -euo pipefail
CRT="${1:?usage: macos.sh <root.crt>}"
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain "$CRT"
echo "installed root CA into System keychain"
echo "note: Firefox uses its own cert store; import $CRT there separately if needed."
