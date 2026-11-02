---
type: module
crate: vortex-neural + worker/
waves: [W10, W12, W17]
updated: 2026-10-31
---
# Neurale layers (framework-eigen netwerken)

Kleine netwerken (MLP's / mini-transformers op embeddings + features), getraind in Python op het event log, geëxporteerd naar ONNX, uitgevoerd in Rust via `ort` op 1060 of CPU in milliseconden.

| Layer | Leert | Basis vóór training |
| --- | --- | --- |
| Router-net | taaktype, moeilijkheid, model, team, strategie | heuristiek + LM-labels |
| Importance scorer | wat opslaan (schrijfpolicy) | heuristiek |
| Reranker | welke herinneringen echt hielpen | Qwen3-Reranker-0.6B, later fine-tune |
| Usefulness-predictor | kans dat herinnering gebruikt wordt | — |
| Entity matcher | dedup / entity resolution | similarity-drempel |
| Verifier / reward model (+ PRM tussenstappen) | goed vs slecht antwoord | LM-Judge |
| Confidence calibrator | echte kans op correctheid | — |
| Decay-predictor | wanneer naar koud geheugen | vaste halfwaardetijd |
| Skill selector | welke skill past | embedding-match |
| Salience-net | belang/verrassing | heuristiek |
| Neural memory module | differentieerbaar associatief geheugen | — (W17) |

## Leerproces
1. Feature logging per beslissing (features, keuze, uitkomst: feedback, test, Judge, gebruikt ja/nee) — vanaf W1 in event log
2. Bootstrap: LM labelt voorbeelden → distillatie
3. Training in idle (PyTorch) → ONNX
4. Schaduwmodus naast huidige versie → promoveren alleen bij betere eval, anders automatisch terug
5. Versiebeheer: dataset, instellingen, scores, regressies
6. Decontaminatie t.o.v. eval-set

Samen met LoRA-lus = "50% van het brein". Zie [[modules/training]].
