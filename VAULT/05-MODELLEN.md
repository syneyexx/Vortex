---
type: referentie
updated: 2026-10-31
bron: webresearch 2026-10-31 (vdf.ai, runaihome, willitrunai, localaimaster, The Batch, Qwen3-Embedding paper)
---
# Modellen

Alles staat in de **model-registry** (config, hot-reload); wisselen = één instelling. Uiteindelijke keuze beslist de eigen eval-suite, niet benchmarks. Modellen op de SSD.

## Altijd geladen — GTX 1060 6 GB
| Rol | Model | Opmerking |
| --- | --- | --- |
| Embeddings | Qwen3-Embedding (0.6B of 4B, GGUF) | Flexibele dimensie (MRL) → 512 dims mogelijk |
| Reranker | Qwen3-Reranker-0.6B | Score 0–1 |
| Router / extractie / claims splitsen | Qwen3.5-2B of 4B | Klein, snel |

## Wisselend — RTX 5060 Ti 16 GB (één tegelijk)
| Taak | Model | Opmerking |
| --- | --- | --- |
| **Standaard ("thuis")**: chat, agents, tools, research | **gpt-oss-20b** (MXFP4, ~12 GB) | Instelbare reasoning, function calling, structured output; ~93K context past; 80–110 tok/s gemeld op 5060 Ti |
| Deep-modus | Qwen3.6-35B-A3B (MoE, ~21 GB Q4) | Experts deels naar 32 GB RAM; slimmer, trager |
| Alternatief general | Gemma 4 26B-A4B | Past krap |
| Code | Devstral Small 2 24B (~15 GB Q4) | KV-cache-quantization verplicht |
| Visie | Qwen3.5-9B | Screenshots, afbeeldingen |
| OCR / documenten | GLM-OCR of PaddleOCR-VL | Tabellen, formules, scans |
| Beeldgeneratie | diffusion-model (nog kiezen) | Alleen als LLM ontladen is |
| Training-basis | Qwen3.5-9B (safetensors) | QLoRA; daarna mergen → GGUF |

## CPU
- Spraak → tekst: Whisper-klasse
- Tekst → spraak: klein TTS-model

## Later / geparkeerd
- Draftmodel voor speculative decoding (alleen als benchmark winst toont)
- Tijdreeksmodel → trading (geparkeerd)

## Wisselen
- NVMe: ~enkele tot ~10 s voor ~12 GB; SATA ~30 s; HDD nooit. **Meten in W0.**
- Trucs: RAM-cache recent model, voorspellend vooraf laden, stappen per model bundelen, KV-cache opslaan/herstellen (llama.cpp slot save/restore), na idle terug naar gpt-oss-20b, wisseltijd zichtbaar in UI.
- MoE-architectuur, MLA, MTP = **selectiecriteria** voor modellen, geen features om te bouwen.

## Sneller, kleiner, minder geheugen (onderzoek 2026-11-01)
Kernfeit: **decode is geheugenbandbreedte-gebonden.** 5060 Ti = 448 GB/s → tok/s ≈ bandbreedte ÷ bytes gelezen per token. Elke truc = minder bytes per token lezen of meer tokens per keer lezen.
| Truc | Wat | Status / voor ons |
| --- | --- | --- |
| MoE met weinig actieve params | alleen ~3–4B per token gelezen | gpt-oss-20b, Qwen3.6-35B-A3B: grootste winst |
| `--moe-cache-mib` (llama.cpp, okt 2026) | LRU-cache van vaak gebruikte experts in VRAM, rest in RAM | +1,3× gemeld op 5060 Ti 16 GB; hit-rate bewaken (≥75%), zelf benchmarken naast `--n-cpu-moe` |
| NVFP4 (Blackwell FP4 tensor cores) | 4-bit float, native op 5060 Ti | llama.cpp ≥ b8967; prefill flink sneller, decode ~gelijk; ~7% kleiner dan Q4_K_M; kwaliteit nog experimenteel → testen op eval |
| KV-cache quantization (q8_0 / FP8) | context-geheugen halveren | standaard aan; meer context in 16 GB |
| Speculative decoding / MTP | meerdere tokens per gewichten-lees | alleen als acceptatie hoog is (meten) |
| EXL3 (trellis) | betere kwaliteit per bit, snellere prefill | alleen als model volledig in VRAM past; GGUF decodeert sneller en ondersteunt offload |
| imatrix / dynamic quants | betere kwaliteit bij zelfde grootte | altijd voorkeur bij GGUF-keuze |
| mmap + OS-filecache | model blijft in RAM-cache → herladen in seconden | onderdeel modelwissel-strategie |
| DFloat11 | lossless ~30% kleiner BF16 | alleen PyTorch-pad (training-basis, eval), niet llama.cpp |
| fastsafetensors | 4,8–7,5× sneller laden safetensors → GPU | getest op Linux/datacenter-GPU's; Windows onzeker → testen bij training |
| 1060 (Pascal) | FP16 extreem traag op deze kaart; int8 (DP4A) wel snel | embeddings/reranker/router als int8-quant GGUF, geen FP16 |

## Trading-noot
LLM = research/nieuws/sentiment/strategie-code; signalen komen van statistiek/tijdreeksmodellen + backtests + harde risicoregels in code. Zie [[GEPARKEERD]].

Zie [[modules/inference-modelbeheer]].
