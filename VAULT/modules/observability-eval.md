---
type: module
dir: evals/ + vortex-core tracing
waves: [W05, W16]
updated: 2026-10-31
---
# Observability en evaluatie

## Observability (W5)
- Trace- en span-ID's door GUI, scheduler, agents, retrieval, tools, inference.
- Wachttijd apart van uitvoering; TTFT, prefill/decode t/s, piek-VRAM, laadtijden.
- Context Assembly Inspector; "waarom deze herinnering opgehaald?"
- Retry-amplification, stall/livelock-detectie.
- Gezondheid per subsysteem (core, DB schrijfbaar, model gereed, index actueel, connector beschikbaar).
- Tool usage analytics, memory usage tracing, error analysis, replay van sessies (uit event log).
- Feedback-knoppen (goed / fout / correctie).

## Eval-suite (W5)
- Vaste eigen taken: redeneren, code (tests), geheugen-recall, research met bronnen, toolgebruik, conflictafhandeling.
- **Retrieval apart**: recall@k, rang, irrelevante passages, vragen zonder antwoord.
- Automatische scoring; elke wijziging ertegen; scores per versie in DB.
- Lekkagecontrole: eval-data niet via geheugen/caches/training/near-duplicates beschikbaar.
- Memory quality metrics; coverage/gaps.
- Voortgangsdashboard "hoe slim is Vortex" per maand en taaktype.

## Geavanceerd (W16)
Record/replay van tools (simulatie vs live gescheiden) · ablation tests · shadow execution (zonder externe effecten) · metamorfische tests · counterfactual sensitivity tests · kalibratiemeting (gescheiden dev/test) · selective-prediction-drempels · A/B van prompts, modellen, strategieën · fault injection (workercrash, volle schijf, verbinding weg, trage tool, kapotte uitvoer, OOM) · positionele contexttests.
