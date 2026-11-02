# W17.10 — GPU-profiling vóór eigen kernels. Regel: alleen een eigen kernel (Triton-prototype →
# CUDA C++ via cudarc of PyTorch-extensie) als Nsight een hotspot aanwijst, met correctheidstest
# t.o.v. de referentie + benchmark, en alleen houden bij gemeten winst. Builds: sm_120 (5060 Ti)
# en sm_61 (1060).
#
# Gebruik (kern gestopt): pwsh scripts/profile-gpu.ps1 [-Seconds 120]
param([int]$Seconds = 120)
$ErrorActionPreference = "Stop"
$out = Join-Path $PSScriptRoot "..\data\profiles"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"

# 1. Tijdlijn van de hele kern onder representatieve last (chat + retrieval + layers + embeddings).
nsys profile --trace=cuda,nvtx,osrt --duration=$Seconds --output "$out\vortex-$stamp" `
  cargo run --release --bin vortex -- --config ..\config serve
# 2. Hete kernels in detail (alleen de top uit stap 1, bv. de embedder op de 1060):
#    ncu --set full --target-processes all -o "$out\kernels-$stamp" <commando>
# 3. Rapport: nsys stats "$out\vortex-$stamp.nsys-rep" --report cuda_gpu_kern_sum
Write-Host "Profiel: $out\vortex-$stamp.nsys-rep — zoek kernels/stappen met > 10% GPU-tijd."
Write-Host "Kandidaten (zie VAULT/waves/W17): neurale layers (gefuseerd), vector-scoring/rerank, batch-embedding pre/post."
