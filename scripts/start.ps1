# Vortex starten (aangeroepen door start_vortex.bat): Python-worker, modelservers (tenzij Vortex ze
# zelf beheert), de kern met de GUI, en daarna de browser. Wat al draait wordt niet opnieuw gestart.
param([switch]$NoBrowser)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$logs = Join-Path $Root "data\logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

function Listening($port) {
  try { $c = New-Object Net.Sockets.TcpClient; $c.Connect("127.0.0.1", $port); $c.Close(); return $true } catch { return $false }
}
function Healthy($url) {
  try { return (Invoke-WebRequest $url -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { return $false }
}
function Toml($text, $key, $default) {
  if ($text -match "(?m)^$key\s*=\s*`"([^`"]*)`"") { return $Matches[1] } else { return $default }
}

$kernel = Join-Path $Root "core\target\release\vortex.exe"
$worker = Join-Path $Root "worker\.venv\Scripts\vortex-worker.exe"
if (-not (Test-Path $kernel) -or -not (Test-Path $worker)) {
  Write-Host "Vortex is nog niet (volledig) geïnstalleerd: draai eerst install.bat" -ForegroundColor Yellow
  exit 1
}
$paths = Get-Content (Join-Path $Root "config\paths.toml") -Raw
$models = Get-Content (Join-Path $Root "config\models.toml") -Raw
$workerAddr = Toml $paths "worker_address" "127.0.0.1:50061"
$workerPort = [int]($workerAddr.Split(":")[-1])

# 1. Python-worker (ingest, OCR, spraak, training, browser).
if (Listening $workerPort) { Write-Host "Worker draait al ($workerAddr)" }
else {
  Write-Host "Worker starten op $workerAddr…"
  Start-Process -FilePath $worker -ArgumentList "--address", $workerAddr -WorkingDirectory (Join-Path $Root "worker") -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $logs "worker.out.log") -RedirectStandardError (Join-Path $logs "worker.err.log")
}

# 2. Modelservers: beheert Vortex ze zelf (managed = true), dan doet de kern dat.
$managed = $models -match "(?m)^\s*managed\s*=\s*true"
$mainUrl = if ($models -match '(?ms)\[servers\.main\].*?url\s*=\s*"([^"]+)"') { $Matches[1] } else { "http://127.0.0.1:8080" }
if ($managed) { Write-Host "Modelservers worden door de kern beheerd (managed = true)" }
elseif (Healthy "$mainUrl/health") { Write-Host "Modelservers draaien al" }
else {
  Write-Host "Modelservers starten…"
  & (Join-Path $PSScriptRoot "models-up.ps1") -LlamaServer (Toml $paths "llama_server" "llama-server") -ModelsDir (Toml $paths "models_dir" "D:\vortex-models")
}

# 3. Kern + GUI (eigen venster met logs; venster sluiten of Ctrl+C = stoppen).
if (Listening 7420) { Write-Host "Kern draait al" }
else {
  Write-Host "Kern starten…"
  $env:VORTEX_EVALS = Join-Path $Root "evals"
  Start-Process -FilePath $kernel -ArgumentList "--config", "config", "serve", "--app-dist", "app\dist" -WorkingDirectory $Root
  $ok = $false
  for ($i = 0; $i -lt 120; $i++) { if (Healthy "http://127.0.0.1:7420/api/status") { $ok = $true; break }; Start-Sleep -Milliseconds 500 }
  if (-not $ok) { Write-Host "De kern reageert niet na 60 s — kijk in het kern-venster." -ForegroundColor Yellow; exit 1 }
}
Write-Host "Vortex draait: http://127.0.0.1:7420" -ForegroundColor Green
if (-not $NoBrowser) { Start-Process "http://127.0.0.1:7420" }
