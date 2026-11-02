---
type: module
crate: vortex-models
waves: [W00, W02, W17]
updated: 2026-10-31
---
# Inference en modelbeheer

## Model-registry (config, hot-reload)
Per model: id, bestand/pad, taakrollen, GPU, VRAM-gewichten, context, KV-type, chat-template, toolcall-parser + versie, capabilities, laadtijd (gemeten), licentie. Zie [[05-MODELLEN]].

## Backend-abstractie
- `Backend`-trait; v1-adapter = OpenAI-compatibel (`llama-server`). Later eigen llama.cpp-bindings (KV-controle, logits, steering, LoRA-hotswap).
- **Capability negotiation**: tools, vision, embeddings, cancellation, logprobs, adapters, formaten. Nooit stilzwijgend doen alsof.
- **Chat-template + tokenizer-contract** per model; rollen, toolberichten, stoptokens gevalideerd.
- **Toolcall-parser per model, geversioneerd**; ongeldige uitvoer = fout, nooit actie.
- **Structured output / constrained generation** (JSON-schema / grammar).

## Lifecycle
gevonden → gevalideerd → laden → opwarmen (readiness: tokenisatie, eerste generatie, protocol) → gereed → uitfaseren (draining) → ontladen. Lopende run houdt model-, template-, adapter- en configversie.

## VRAM en wisselen
- Admission control; FlashAttention aan; KV-cache-quantization.
- Rolverdeling vast: 5060 Ti wisselt, 1060 vast team.
- Wisselen: RAM-cache, voorspellend laden (router ziet volgende stap), bundelen, KV save/restore, terug naar thuismodel na idle, wisseltijd zichtbaar.
- MoE expert-offload naar RAM voor Deep-model.

## Metingen (naar tracing)
Wachttijd, time-to-first-token, prefill t/s, decode t/s, inter-token-tijd, piek-VRAM, laadtijd. Numerieke fouten (NaN/inf) = runtimefout.

## Streaming
Incrementele UTF-8-decodering; stopsequenties over fragmentgrenzen; toolargumenten streamen maar pas uitvoeren als volledig + geldig.

## Later (W17)
Speculative decoding (alleen bij gemeten winst), chunked prefill / continuous batching (bij eigen backend), logprob-diagnostiek, positionele contexttests per model, optionele cloud-escalatie (alleen als gebruiker het aanzet).

Verbindingen: [[modules/runtime-core]] (scheduler), [[modules/neurale-layers]] (router kiest model), [[modules/gui]] (Models-pagina, statusbalk).
