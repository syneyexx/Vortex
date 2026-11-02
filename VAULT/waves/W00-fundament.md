---
wave: W00
status: open
modules: [inference-modelbeheer, runtime-core]
---
# W00 — Fundament

## Checks
- [~] `W00.01` SSD: **NVMe 250 GB** (bevestigd 2026-11-02); nog noteren: Windows erop?, vrije ruimte, HDD-groottes + schijfletters → [[04-HARDWARE-EN-OPSLAG]]
- [ ] `W00.02` NVIDIA-driver; `nvidia-smi` toont 5060 Ti + 1060
- [ ] `W00.03` llama.cpp-build die Blackwell én Pascal ondersteunt (anders twee builds)
- [ ] `W00.04` CUDA-toolchain kiezen: CUDA 13 ondersteunt niets onder Turing (1060 valt af); 5060 Ti (sm_120) vraagt CUDA 12.8+. Verwacht: CUDA 12.8/12.9 voor beide (sm_61 + sm_120) — verifiëren met een testkernel op beide kaarten. PyTorch-wheel voor Pascal verifiëren vóór installeren
- [ ] `W00.05` Laadtijd per model gemeten (in [[05-MODELLEN]]); TTFT en t/s gemeten → prestatiedoelen vastzetten in [[modules/orkestrator-performance]]
- [ ] `W00.06` WSL2 installeren en Unsloth/PyTorch-training kort testen: Windows native vs WSL2 (snelheid, VRAM, stabiliteit) → besluit in [[03-BESLUITEN]]

## Installeren
- [ ] `W00.07` Git, Rust (rustup), Node LTS + pnpm, Python 3.12 + uv, VS Build Tools (C++), WebView2/Tauri-vereisten, protoc, `just`
- [ ] `W00.08` SearXNG (Docker of native) — kan ook in W08

## Repo
- [ ] `W00.09` Repo `syneyexx/Vortex` geklond op de SSD (`C:\dev\Vortex`, buiten OneDrive); grote data-paden op HDD vastgelegd in `config/` en [[04-HARDWARE-EN-OPSLAG]]
- [ ] `W00.10` Rust-workspace met alle crates uit [[20-CODEMAP]] (incl. `vortex-types`, `vortex-crawler`), leeg maar compilerend, afhankelijkheidsrichting afgedwongen
- [ ] `W00.11` `proto/` met eerste contracten (kern ⇄ worker, kern ⇄ GUI) volgens [[15-CONTRACTEN]]; codegen voor Rust, Python en TS
- [ ] `W00.12` Python-project `worker/` (uv), Tauri-app `app/`
- [ ] `W00.13` `config/` (models.toml, prompts/, roles/, skills/, paths.toml), `evals/`, `design/`; `CARD.md` per crate (template in `VAULT/templates/`)
- [ ] `W00.14` `justfile` met vaste commando's: `build`, `test`, `check` (fmt + clippy + ruff + eslint + `vault.py check`), `run`, `eval`, `models-up`, `models-down`
- [ ] `W00.15` `vortex` CLI (binary in `vortex-server` of eigen crate) — begin: `vortex status` (GPU's, modellen, health); groeit per wave mee (eval, model laden, runs bekijken/stoppen)
- [ ] `W00.16` Lint/format: clippy, rustfmt, ruff, eslint/prettier

## Modellen
- [ ] `W00.17` Downloaden naar SSD: gpt-oss-20b (GGUF), Qwen3-Embedding, Qwen3-Reranker-0.6B, Qwen3.5-2B/4B
- [ ] `W00.18` `llama-server` op 5060 Ti (gpt-oss-20b, FlashAttention, KV-quant)
- [ ] `W00.19` `llama-server` op 1060 (embedding + reranker + router)
- [ ] `W00.20` Start/stop-scripts (`just models-up/down`); health-endpoint getest

## Gate
Beide servers reageren, readiness-test slaagt, metingen (TTFT, t/s, VRAM, laadtijd) genoteerd, repo compileert volledig, `just check` groen, `vortex status` toont beide GPU's en modellen.
