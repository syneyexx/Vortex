---
wave: W02
status: bezig  # code-deel af 2026-11-02; metingen/echte modellen op de PC
modules: [inference-modelbeheer]
---
# W02 — Modelbeheer
Zie [[modules/inference-modelbeheer]], [[05-MODELLEN]].

- [x] `W02.01` Model-registry (models.toml) met alle velden, hot-reload, validatie
- [~] `W02.02` `Backend`-trait + OpenAI-compatibele adapter (chat, stream, embeddings, rerank) — code af, getest tegen nep-server (chat, stream, embeddings, rerank, annuleren) — **verifiëren met echte llama-server op de PC**
- [x] `W02.03` Capability negotiation per backend
- [~] `W02.04` Chat-template- en tokenizer-contract per model + validatie — template-contract via `/props` (geen template = fout; geen tool-rol = tools uit) — **verifiëren per echt model**
- [x] `W02.05` Toolcall-parser per model met versie; ongeldig = fout-event — **af:** `openai-native/v1` + `hermes-xml/v1`, schema-validatie; het `tool.call.invalid`-event wordt gezet door de toollaag (W08)
- [~] `W02.06` Structured output / grammar-constrained generation — json_schema/json_object/GBNF verstuurd + JSON-controle — **verifiëren op echte server**
- [~] `W02.07` Lifecycle manager (7 fasen) + readiness-checks + draining — 7 fasen + readiness (tokenize, eerste generatie, protocol) + draining, getest met test-dubbel — **echte modellen op de PC**
- [~] `W02.08` Wisselen op 5060 Ti: laden/ontladen, RAM-cache, voorspellend laden, bundelen, terug naar thuismodel na idle — wisselen, terug naar thuismodel na idle, bestand voorverwarmen (OS-cache) af; bundelen = model-affiniteit in de Dirigent (W04) — **wisseltijden meten op de PC**
- [~] `W02.09` KV-cache save/restore bij wissel — `/slots` save/restore bij wissel, getest met test-dubbel — **verifiëren met llama-server `--slot-save-path`**
- [ ] `W02.10` MoE-expert-offload-profiel voor Deep-model: benchmark `--n-cpu-moe` vs `-cmoe` + `--moe-cache-mib` (hit-rate in tracing)
- [ ] `W02.11` Quant-shootout per model op eigen eval: Q4_K_M (imatrix) vs NVFP4 (llama.cpp ≥ b8967) vs evt. EXL3; kiezen op kwaliteit × tok/s × VRAM
- [ ] `W02.12` 1060-team als int8-quants (geen FP16 op Pascal)
- [ ] `W02.13` Speculative decoding/MTP proberen waar model het ondersteunt; alleen aan bij gemeten winst
- [x] `W02.14` Incrementele UTF-8 + stopsequenties over fragmentgrenzen + toolargumenten-fasen
- [~] `W02.15` Metingen naar tracing: wachttijd, TTFT, prefill/decode t/s, inter-token, piek-VRAM, laadtijd; NaN/inf-detectie — TTFT, wacht, prefill/decode t/s, inter-token, NaN/inf af; **piek-VRAM (nvidia-smi) nog toevoegen op de PC**
- [ ] `W02.16` Specialisten geregistreerd en getest: Deep, Code, Visie (laden + readiness)

## Gate
Elk geregistreerd model laadt, warmt op, wisselt en ontlaadt foutloos; metingen zichtbaar; parserfouten worden nooit acties.
