---
wave: W02
status: open
modules: [inference-modelbeheer]
---
# W02 — Modelbeheer
Zie [[modules/inference-modelbeheer]], [[05-MODELLEN]].

- [ ] `W02.01` Model-registry (models.toml) met alle velden, hot-reload, validatie
- [ ] `W02.02` `Backend`-trait + OpenAI-compatibele adapter (chat, stream, embeddings, rerank)
- [ ] `W02.03` Capability negotiation per backend
- [ ] `W02.04` Chat-template- en tokenizer-contract per model + validatie
- [ ] `W02.05` Toolcall-parser per model met versie; ongeldig = fout-event
- [ ] `W02.06` Structured output / grammar-constrained generation
- [ ] `W02.07` Lifecycle manager (7 fasen) + readiness-checks + draining
- [ ] `W02.08` Wisselen op 5060 Ti: laden/ontladen, RAM-cache, voorspellend laden, bundelen, terug naar thuismodel na idle
- [ ] `W02.09` KV-cache save/restore bij wissel
- [ ] `W02.10` MoE-expert-offload-profiel voor Deep-model: benchmark `--n-cpu-moe` vs `-cmoe` + `--moe-cache-mib` (hit-rate in tracing)
- [ ] `W02.11` Quant-shootout per model op eigen eval: Q4_K_M (imatrix) vs NVFP4 (llama.cpp ≥ b8967) vs evt. EXL3; kiezen op kwaliteit × tok/s × VRAM
- [ ] `W02.12` 1060-team als int8-quants (geen FP16 op Pascal)
- [ ] `W02.13` Speculative decoding/MTP proberen waar model het ondersteunt; alleen aan bij gemeten winst
- [ ] `W02.14` Incrementele UTF-8 + stopsequenties over fragmentgrenzen + toolargumenten-fasen
- [ ] `W02.15` Metingen naar tracing: wachttijd, TTFT, prefill/decode t/s, inter-token, piek-VRAM, laadtijd; NaN/inf-detectie
- [ ] `W02.16` Specialisten geregistreerd en getest: Deep, Code, Visie (laden + readiness)

## Gate
Elk geregistreerd model laadt, warmt op, wisselt en ontlaadt foutloos; metingen zichtbaar; parserfouten worden nooit acties.
