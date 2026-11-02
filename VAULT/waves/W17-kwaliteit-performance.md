---
wave: W17
status: open
modules: [observability-eval, inference-modelbeheer, tools-skills, cognitie]
---
# W17 — Kwaliteit en performance

- [ ] `W17.01` Record/replay voor tooltests (simulatie vs live)
- [ ] `W17.02` Ablation tests per component; overbodige complexiteit verwijderen — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7)
- [ ] `W17.03` Shadow execution van nieuwe configs
- [ ] `W17.04` Metamorfische + counterfactual tests
- [ ] `W17.05` A/B van prompts, modellen, strategieën
- [ ] `W17.06` Fault injection (crash, volle schijf, verbinding weg, trage tool, kapotte uitvoer, OOM) — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (kleine fault-tests per component mogen wel)
- [ ] `W17.07` Positionele contexttests per model
- [ ] `W17.08` Speculative decoding (alleen bij gemeten winst), chunked prefill/continuous batching bij eigen backend, logprob-diagnostiek
- [ ] `W17.09` Eigen llama.cpp-bindings indien nodig (KV, logits, LoRA-hotswap)
- [ ] `W17.10` Eigen GPU-kernels, alleen waar profiling (Nsight Systems/Compute) een hotspot aanwijst: gefuseerde kernels voor neurale layers, GPU-scoring voor vector search/rerank, batch-embedding-pre/postprocessing. Prototype in Triton (Python), productie in CUDA C++ aangeroepen vanuit Rust (cudarc) of als PyTorch-extensie. Elke kernel: correctheidstest t.o.v. referentie + benchmark; alleen houden bij gemeten winst. Twee builds: sm_120 (5060 Ti) en sm_61 (1060)
- [ ] `W17.11` Semantische antwoordcache (strikte checks, nooit voor acties), cache-stampede-preventie
- [ ] `W17.12` Incremental recomputation, common subexpression elimination, value-of-information planning, cost-based plan selection (kosten uit eigen metingen)
- [ ] `W17.13` Programmatic tool calling
- [ ] `W17.14` Browser automation, git/repo-tools, computer use, repo-niveau codebegrip
- [ ] `W17.15` **Experiment (gebruiker wil testen):** quantum-inspired tensor-netwerk-compressie (MPO, CompactifAI-achtig: SVD per gewichtenmatrix + korte "healing"-training) — **alleen op eigen neurale layers en hulpmodellen (router/extractie), NOOIT op het redeneermodel** (besluit 2026-11-01: redeneren gaat aantoonbaar omlaag). Vergelijken met gewone quantisatie op kwaliteit × snelheid × VRAM
- [ ] `W17.16` Visuele UI-controle (screenshot vs referentie)
- [ ] `W17.17` Optionele cloud-escalatie (alleen als gebruiker aanzet)

## Gate
🔒 Ablations tonen per component meetbare waarde; fault injection herstelt altijd correct (beide op commando).
