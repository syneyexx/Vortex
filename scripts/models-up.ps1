# Start beide llama-servers (W00.20). Gebruik: pwsh scripts/models-up.ps1
# Leest paden uit config/paths.toml en config/models.toml niet automatisch: pas onderstaande waarden aan
# of geef ze mee: -LlamaServer C:\tools\llama.cpp\llama-server.exe -ModelsDir D:\vortex-models
param(
  [string]$LlamaServer = "C:\tools\llama.cpp\llama-server.exe",
  [string]$ModelsDir = "D:\vortex-models",
  [int]$MainGpu = 0,
  [int]$AuxGpu = 1
)
$ErrorActionPreference = "Stop"
$logs = Join-Path $PSScriptRoot "..\data\logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

function Start-Llama($name, $gpu, $port, $args) {
  $env:CUDA_VISIBLE_DEVICES = "$gpu"
  $p = Start-Process -FilePath $LlamaServer -ArgumentList $args -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $logs "$name.out.log") -RedirectStandardError (Join-Path $logs "$name.err.log")
  Set-Content -Path (Join-Path $logs "$name.pid") -Value $p.Id
  Write-Host "$name gestart (pid $($p.Id), GPU $gpu, poort $port)"
}

# Hoofd-GPU: gpt-oss-20b, FlashAttention, KV-cache q8_0.
Start-Llama "main" $MainGpu 8080 @(
  "-m", (Join-Path $ModelsDir "gpt-oss-20b-mxfp4.gguf"), "--port", "8080", "--host", "127.0.0.1",
  "-ngl", "999", "-c", "32768", "-fa", "on", "--cache-type-k", "q8_0", "--cache-type-v", "q8_0", "--jinja"
)
# 1060-team (int8, één model per proces): embeddings, reranker, router.
Start-Llama "aux" $AuxGpu 8081 @(
  "-m", (Join-Path $ModelsDir "qwen3-embedding-0.6b-q8_0.gguf"), "--port", "8081", "--host", "127.0.0.1",
  "-ngl", "999", "--embedding", "--pooling", "last", "-c", "8192"
)
Start-Llama "rerank" $AuxGpu 8082 @(
  "-m", (Join-Path $ModelsDir "qwen3-reranker-0.6b-q8_0.gguf"), "--port", "8082", "--host", "127.0.0.1",
  "-ngl", "999", "--reranking", "-c", "8192"
)
Start-Llama "router" $AuxGpu 8083 @(
  "-m", (Join-Path $ModelsDir "qwen3.5-2b-q8_0.gguf"), "--port", "8083", "--host", "127.0.0.1",
  "-ngl", "999", "--jinja", "-c", "8192"
)

# Wachten op /health.
foreach ($port in 8080, 8081, 8082, 8083) {
  $ok = $false
  for ($i = 0; $i -lt 120; $i++) {
    try { if ((Invoke-WebRequest "http://127.0.0.1:$port/health" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200) { $ok = $true; break } } catch {}
    Start-Sleep -Milliseconds 500
  }
  if ($ok) { Write-Host "poort $port gereed" } else { Write-Warning "poort $port niet gereed na 60 s — zie data\logs" }
}
