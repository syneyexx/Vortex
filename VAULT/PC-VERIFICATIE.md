---
type: checklist
updated: 2026-11-02
---
# PC-verificatie — wat alleen op de echte PC kan

Alles hieronder is gebouwd en in de cloud getest met nep-modellen/test-engines; op de PC wordt het
echt gemeten. Per item: wave-item van `[~]`/`[ ]` naar `[x]` + meetwaarde in [[LOG]] (of [[05-MODELLEN]]).
🔒 = grote test, alleen na "GROTE TESTEN NU".

## A. Basis en installatie
- [ ] `install.bat` volledig groen (zie [[OVERDRACHT]] §1 voor de open pnpm-fout) — W00.07, W00.12
- [ ] Driver + `nvidia-smi` toont 5060 Ti + 1060 — W00.02; CUDA-keuze (12.8, beide GPU's) — W00.04
- [ ] llama.cpp-build voor Blackwell én Pascal — W00.03; servers op beide GPU's — W00.18, W00.19, W00.20
- [ ] Repo op SSD, paden in `config/paths.toml` / Instellingen → Mappen — W00.09, W00.01
- [ ] Modellen gedownload — W00.17; registry + specialisten laden — W02.16
- [ ] CLI `vortex status` en `vortex serve` — W00.15; secrets via Windows Credential Manager — W01.14
- [ ] Subprocess-supervisie + Job Objects (Windows) — W01.15, W04.03; tokio-runtimes/CPU-affiniteit — W04.02
- [ ] SearXNG (Docker of native) voor web-research — W00.08
- [ ] WSL2 vs native voor training — W00.06

## B. Modellen en inferentie (meten)
- [ ] Laadtijd, TTFT, t/s per model → [[05-MODELLEN]] — W00.05, W02.15
- [ ] Backend-trait tegen echte llama-server (chat, stream, embeddings, rerank) — W02.02, W02.04, W02.06
- [ ] Lifecycle, wisselen, KV save/restore op de 5060 Ti — W02.07, W02.08, W02.09
- [ ] MoE-offload, quant-shootout, int8 op 1060 — W02.10, W02.11, W02.12
- [ ] Speculative decoding: `vortex specbench --model …` → `spec_gain` in models.toml — W02.13, W17.08
- [ ] GPU-bewaker: `managed = true`, game starten → GPU vrij, game weg → modellen terug — W15.01
- [ ] Embedding-service op de 1060 + LanceDB/vectorindex-keuze — W05.02, W05.03

## C. GUI en chat
- [ ] Chat end-to-end, stoppen, coalescing, statusbalk met beide GPU's live — W03.03, W03.08, W03.11, W03.12
- [ ] `just visual` (screenshots vs referentie) na eigen wijzigingen

## D. Functies met echte modellen/diensten
- [ ] Wereldmonitor live (RSS/GDELT/USGS/…), Wikidata — W14.01; HF-datasets streamen — W14.09
- [ ] Visie met ROI (model met rol `vision` + `mmproj`) — W16.01; OCR (PaddleOCR/Tesseract) — W16.02
- [ ] ColPali-paginaretrieval (GPU) — W16.04; spraak (faster-whisper + Piper/Kokoro) — W16.05
- [ ] Beeldgeneratie (diffusers, LLM wordt ontladen) — W16.06
- [ ] Reranker-fine-tune — W11.06; QLoRA-training op de PC — W13.02
- [ ] Telefoon via Tailscale (`[remote] tailscale = true`) — W15.08
- [ ] Computer use (`computer`-tool, bureaublad) — W17.14
- [ ] GPU-profiling (`scripts/profile-gpu.ps1`), eigen kernels alleen bij hotspot — W17.10

## E. 🔒 Grote tests (alleen op commando)
- [ ] 🔒 Performance-doelen `vortex perf` — W06.11
- [ ] 🔒 24-uurs soak `vortex soak` — W06.12
- [ ] 🔒 Tiering TB-simulatie `vortex tierbench` — W12.07
- [ ] 🔒 Fault injection `vortex faults` — W17.06; ablations `vortex ablate` — W17.02
- [ ] 🔒 Gates met meerdaagse/weekruns — W14 (wereldmonitor dagen), W15 (week zelfstandig)
