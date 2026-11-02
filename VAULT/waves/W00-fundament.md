---
wave: W00
status: open
modules: [inference-modelbeheer, runtime-core]
---
# W00 — Fundament

## Checks
- [ ] `W00.01` SSD: NVMe of SATA, OS erop?, vrije ruimte genoteerd in [[04-HARDWARE-EN-OPSLAG]]
- [ ] `W00.02` NVIDIA-driver; `nvidia-smi` toont 5060 Ti + 1060
- [ ] `W00.03` llama.cpp-build die Blackwell én Pascal ondersteunt (anders twee builds)
- [ ] `W00.04` CUDA-toolchain kiezen: CUDA 13 ondersteunt niets onder Turing (1060 valt af); 5060 Ti (sm_120) vraagt CUDA 12.8+. Verwacht: CUDA 12.8/12.9 voor beide (sm_61 + sm_120) — verifiëren met een testkernel op beide kaarten. PyTorch voor de 1060: laatste Pascal-wheels = 2.14 (cu126)
- [ ] `W00.05` Laadtijd per model gemeten (in [[05-MODELLEN]])

## Installeren
- [ ] `W00.06` Git, Rust (rustup), Node LTS + pnpm, Python 3.11/3.12 + uv, VS Build Tools (C++), WebView2/Tauri-vereisten, protoc
- [ ] `W00.07` SearXNG (Docker of native) — kan ook in W07

## Repo
- [ ] `W00.08` repo `syneyexx/Vortex` geklond op de SSD, structuur uit [[10-ARCHITECTUUR]] / [[20-CODEMAP]]
- [ ] `W00.09` Rust-workspace met alle crates (lege maar compilerende modules)
- [ ] `W00.10` Python-project `worker/` (uv), Tauri-app `app/`
- [ ] `W00.11` `config/` (models.toml, prompts/, roles/, skills/), `evals/`, `design/`; `CARD.md` per crate (template in `VAULT/templates/`)
- [ ] `W00.12` Lint/format: clippy, rustfmt, ruff, eslint/prettier; CI-script lokaal

## Modellen
- [ ] `W00.13` Downloaden naar SSD: gpt-oss-20b (GGUF), Qwen3-Embedding, Qwen3-Reranker-0.6B, Qwen3.5-2B/4B
- [ ] `W00.14` `llama-server` op 5060 Ti (gpt-oss-20b, FlashAttention, KV-quant)
- [ ] `W00.15` `llama-server` op 1060 (embedding + reranker + router)
- [ ] `W00.16` Start/stop-scripts; health-endpoint getest

## Gate
Beide servers reageren, readiness-test slaagt, metingen (TTFT, t/s, VRAM) genoteerd, repo compileert volledig.
