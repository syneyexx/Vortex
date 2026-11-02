# Stop de llama-servers die models-up.ps1 startte.
$logs = Join-Path $PSScriptRoot "..\data\logs"
foreach ($name in "main", "aux", "rerank", "router") {
  $f = Join-Path $logs "$name.pid"
  if (Test-Path $f) {
    $procId = Get-Content $f
    Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
    Remove-Item $f
    Write-Host "$name gestopt (pid $procId)"
  }
}
