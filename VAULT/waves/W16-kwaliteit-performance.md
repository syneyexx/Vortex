---
wave: W16
status: open
modules: [observability-eval, inference-modelbeheer, tools-skills, cognitie]
---
# W16 — Kwaliteit en performance

- [ ] `W16.01` Record/replay voor tooltests (simulatie vs live)
- [ ] `W16.02` Ablation tests per component; overbodige complexiteit verwijderen
- [ ] `W16.03` Shadow execution van nieuwe configs
- [ ] `W16.04` Metamorfische + counterfactual tests
- [ ] `W16.05` A/B van prompts, modellen, strategieën
- [ ] `W16.06` Fault injection (crash, volle schijf, verbinding weg, trage tool, kapotte uitvoer, OOM)
- [ ] `W16.07` Positionele contexttests per model
- [ ] `W16.08` Speculative decoding (alleen bij gemeten winst), chunked prefill/continuous batching bij eigen backend, logprob-diagnostiek
- [ ] `W16.09` Eigen llama.cpp-bindings indien nodig (KV, logits, LoRA-hotswap)
- [ ] `W16.10` Eigen GPU-kernels, alleen waar profiling (Nsight Systems/Compute) een hotspot aanwijst: gefuseerde kernels voor neurale layers, GPU-scoring voor vector search/rerank, batch-embedding-pre/postprocessing. Prototype in Triton (Python), productie in CUDA C++ aangeroepen vanuit Rust (cudarc) of als PyTorch-extensie. Elke kernel: correctheidstest t.o.v. referentie + benchmark; alleen houden bij gemeten winst. Twee builds: sm_120 (5060 Ti) en sm_61 (1060)
- [ ] `W16.11` Semantische antwoordcache (strikte checks, nooit voor acties), cache-stampede-preventie
- [ ] `W16.12` Incremental recomputation, common subexpression elimination, value-of-information planning, cost-based plan selection (kosten uit eigen metingen)
- [ ] `W16.13` Programmatic tool calling
- [ ] `W16.14` Browser automation, git/repo-tools, computer use, repo-niveau codebegrip
- [ ] `W16.15` **Experiment (gebruiker wil testen):** quantum-inspired tensor-netwerk-compressie (MPO, CompactifAI-achtig: SVD per gewichtenmatrix + korte "healing"-training) — **alleen op eigen neurale layers en hulpmodellen (router/extractie), NOOIT op het redeneermodel** (besluit 2026-11-01: redeneren gaat aantoonbaar omlaag). Vergelijken met gewone quantisatie op kwaliteit × snelheid × VRAM
- [ ] `W16.16` Visuele UI-controle (screenshot vs referentie)
- [ ] `W16.17` Optionele cloud-escalatie (alleen als gebruiker aanzet)

## Gate
Ablations tonen per component meetbare waarde; fault injection herstelt altijd correct.
