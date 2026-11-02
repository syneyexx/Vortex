# Vortex installeren op Windows (aangeroepen door install.bat).
# Installeert wat ontbreekt (via winget), bouwt de Rust-kern, maakt de Python-omgeving van de
# worker met alle extra's, installeert de GUI-dependencies en bouwt de GUI. Opnieuw draaien mag:
# wat er al is wordt overgeslagen of bijgewerkt. Log: install.log in de repo-map.
param(
  [switch]$Gpu,        # CUDA-PyTorch + training/beeldgeneratie (zonder vraag)
  [switch]$NoGpu,      # GPU-pakketten overslaan (zonder vraag)
  [switch]$SkipTools   # geen winget-installaties (alleen bouwen)
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
Start-Transcript -Path (Join-Path $Root "install.log") -Append | Out-Null
$failed = New-Object System.Collections.Generic.List[string]
# uv-cache (C:) en repo op een andere schijf: hardlinken kan niet → direct kopiëren (geen waarschuwing).
$env:UV_LINK_MODE = "copy"

function Say($t) { Write-Host ""; Write-Host "==> $t" -ForegroundColor Cyan }
function Ok($t) { Write-Host "    $t" -ForegroundColor Green }
function Warn($t) { Write-Host "    $t" -ForegroundColor Yellow }
function Refresh-Path {
  $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")
  $cargo = Join-Path $env:USERPROFILE ".cargo\bin"
  if (Test-Path $cargo) { $env:Path = "$cargo;$env:Path" }
}
function Has($cmd) { return [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }
function Step($name, [scriptblock]$body) {
  Say $name
  try { & $body; Ok "klaar" } catch { Warn "MISLUKT: $($_.Exception.Message)"; $failed.Add($name) }
}
function Run($exe, [string[]]$argv) {
  & $exe @argv
  if ($LASTEXITCODE -ne 0) { throw "$exe $($argv -join ' ') gaf exitcode $LASTEXITCODE" }
}
function Install-Pkg($id, $what) {
  Write-Host "    installeren: $what ($id)"
  & winget.exe install --id $id -e --accept-package-agreements --accept-source-agreements --silent | Out-Host
  Refresh-Path
}

Write-Host "Vortex installeren in $Root" -ForegroundColor White
Refresh-Path

# ── 1. Programma's ────────────────────────────────────────────────────────────
if (-not $SkipTools) {
  Step "Programma's controleren (winget)" {
    if (-not (Has "winget")) { throw "winget ontbreekt: installeer 'App Installer' uit de Microsoft Store en start opnieuw" }
    if (-not (Has "git")) { Install-Pkg "Git.Git" "Git" } else { Ok "git aanwezig" }
    if (-not (Has "python") -or -not ((& python --version 2>&1) -match "3\.1[1-3]")) { Install-Pkg "Python.Python.3.12" "Python 3.12" } else { Ok "python aanwezig" }
    if (-not (Has "uv")) { Install-Pkg "astral-sh.uv" "uv (Python-pakketbeheer)" } else { Ok "uv aanwezig" }
    if (-not (Has "node")) { Install-Pkg "OpenJS.NodeJS.LTS" "Node.js LTS" } else { Ok "node aanwezig" }
    if (-not (Has "rustup")) { Install-Pkg "Rustlang.Rustup" "Rust (rustup)" } else { Ok "rustup aanwezig" }
    if (-not (Has "ffmpeg")) { Install-Pkg "Gyan.FFmpeg" "FFmpeg (audio/video)" } else { Ok "ffmpeg aanwezig" }
    if (-not (Has "just")) { Install-Pkg "Casey.Just" "just (vaste commando's)" } else { Ok "just aanwezig" }
    if (-not (Has "tesseract")) { Install-Pkg "UB-Mannheim.TesseractOCR" "Tesseract (OCR-terugval)" } else { Ok "tesseract aanwezig" }
  }
  Step "C++ build tools (nodig voor Rust op Windows)" {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $hasVc = (Test-Path $vswhere) -and ((& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath) -ne $null)
    if ($hasVc) { Ok "MSVC aanwezig" } else {
      Write-Host "    installeren: Visual Studio Build Tools (C++) — kan even duren"
      & winget.exe install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" | Out-Host
    }
  }
  Step "llama.cpp (llama-server voor de modellen)" {
    $paths = Get-Content (Join-Path $Root "config\paths.toml") -Raw
    $ls = if ($paths -match 'llama_server\s*=\s*"([^"]+)"') { $Matches[1] } else { "" }
    if ($ls -and (Test-Path $ls)) { Ok "llama-server gevonden: $ls" }
    elseif (Has "llama-server") { Ok "llama-server in PATH" }
    else {
      Install-Pkg "ggml.llamacpp" "llama.cpp"
      if (Has "llama-server") { Warn "zet in config\paths.toml: llama_server = `"$((Get-Command llama-server).Source -replace '\\','/')`"" }
      else { Warn "llama-server niet gevonden: download een CUDA-build van https://github.com/ggml-org/llama.cpp/releases en zet het pad in config\paths.toml" }
    }
  }
}
Refresh-Path

# ── 2. Rust-kern ──────────────────────────────────────────────────────────────
Step "Rust-toolchain" {
  Run "rustup" @("default", "stable")
  Run "rustup" @("update", "stable", "--no-self-update")
}
Step "Rust-kern bouwen (release, eerste keer ~10-20 min)" {
  Push-Location (Join-Path $Root "core")
  try { Run "cargo" @("build", "--release", "-p", "vortex-server") } finally { Pop-Location }
}

# ── 3. Python-worker ──────────────────────────────────────────────────────────
Step "Python-omgeving van de worker (worker\.venv, alle extra's)" {
  Push-Location (Join-Path $Root "worker")
  try {
    Run "uv" @("venv", ".venv", "--python", "3.12", "--allow-existing")
    Run "uv" @("sync", "--extra", "ingest", "--extra", "layers", "--extra", "hf", "--extra", "media", "--extra", "automation")
  } finally { Pop-Location }
}
Step "PaddleOCR-backend (optioneel; anders Tesseract)" {
  Push-Location (Join-Path $Root "worker")
  try { & uv pip install --python .venv\Scripts\python.exe paddlepaddle | Out-Host; if ($LASTEXITCODE -ne 0) { Warn "paddlepaddle niet geïnstalleerd: OCR gebruikt Tesseract" } } finally { Pop-Location }
}
Step "Browser voor automatisering (Playwright Chromium)" {
  Run (Join-Path $Root "worker\.venv\Scripts\python.exe") @("-m", "playwright", "install", "chromium")
}
$doGpu = $Gpu
if (-not $Gpu -and -not $NoGpu) {
  $a = Read-Host "GPU-pakketten installeren (CUDA-PyTorch, QLoRA-training, beeldgeneratie, ColPali; ~6 GB)? [J/n]"
  $doGpu = ($a -eq "" -or $a -match "^[jJyY]")
}
if ($doGpu) {
  Step "GPU-pakketten (CUDA 12.8-PyTorch voor RTX 50-serie + training/beeld)" {
    Push-Location (Join-Path $Root "worker")
    try {
      Run "uv" @("pip", "install", "--python", ".venv\Scripts\python.exe", "--reinstall-package", "torch", "torch", "torchvision", "torchaudio", "--index-url", "https://download.pytorch.org/whl/cu128")
      Run "uv" @("pip", "install", "--python", ".venv\Scripts\python.exe", "transformers", "peft", "trl", "datasets", "huggingface_hub", "accelerate", "bitsandbytes", "diffusers", "colpali-engine", "onnx")
    } finally { Pop-Location }
  }
}

# ── 4. GUI ────────────────────────────────────────────────────────────────────
Step "GUI-dependencies (pnpm)" {
  Push-Location (Join-Path $Root "app")
  try {
    if (-not (Has "pnpm")) { try { Run "corepack" @("enable"); Refresh-Path } catch { Warn "corepack enable lukte niet (geen adminrechten?): npm-terugval" } }
    if (-not (Has "pnpm")) { Run "npm" @("install", "-g", "pnpm"); Refresh-Path }
    Run "pnpm" @("install", "--frozen-lockfile")
  } finally { Pop-Location }
}
Step "GUI bouwen (app\dist)" {
  Push-Location (Join-Path $Root "app")
  try { Run "pnpm" @("build") } finally { Pop-Location }
}

# ── 5. Mappen en config ──────────────────────────────────────────────────────
Step "Mappen en modellen controleren" {
  New-Item -ItemType Directory -Force -Path (Join-Path $Root "data\logs") | Out-Null
  $paths = Get-Content (Join-Path $Root "config\paths.toml") -Raw
  foreach ($key in "models_dir", "cold_dir") {
    if ($paths -match "(?m)^$key\s*=\s*`"([^`"]+)`"") {
      $p = $Matches[1]
      $drive = Split-Path -Qualifier $p -ErrorAction SilentlyContinue
      if ($drive -and -not (Test-Path "$drive\")) { Warn "${key} = ${p}: schijf $drive bestaat niet — pas config\paths.toml aan" }
      else { New-Item -ItemType Directory -Force -Path $p | Out-Null; Ok "${key}: $p" }
    }
  }
  $models = Get-Content (Join-Path $Root "config\models.toml") -Raw
  $missing = @()
  foreach ($m in [regex]::Matches($models, '(?m)^file\s*=\s*"([^"]+)"')) { if (-not (Test-Path $m.Groups[1].Value)) { $missing += $m.Groups[1].Value } }
  if ($missing.Count -gt 0) {
    Warn "Deze modelbestanden ontbreken nog (download de GGUF's, bv. van Hugging Face, of pas config\models.toml aan):"
    $missing | ForEach-Object { Warn "  $_" }
  } else { Ok "alle modelbestanden gevonden" }
}

Write-Host ""
if ($failed.Count -eq 0) {
  Write-Host "Installatie klaar. Start Vortex met start_vortex.bat" -ForegroundColor Green
} else {
  Write-Host "Installatie klaar met problemen in: $($failed -join ', ')" -ForegroundColor Yellow
  Write-Host "Zie install.log; los het op en draai install.bat opnieuw (wat al klaar is gaat snel)." -ForegroundColor Yellow
}
Stop-Transcript | Out-Null
if ($failed.Count -gt 0) { exit 1 }
