# Vortex stoppen: kern, worker en de modelservers die start_vortex.bat startte.
$Root = Split-Path -Parent $PSScriptRoot
Get-Process vortex -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process vortex-worker -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
& (Join-Path $PSScriptRoot "models-down.ps1")
Write-Host "Vortex gestopt."
