# Install the broccoli LAN root CA into the Windows Local Machine Root store.
# Run in an elevated PowerShell. TARGET-SIDE: no network.
param([Parameter(Mandatory=$true)][string]$CertPath)
$ErrorActionPreference = "Stop"
certutil -addstore -f Root $CertPath
Write-Host "installed root CA into Local Machine Root store"
Write-Host "note: Firefox uses its own cert store; import $CertPath there separately if needed."
