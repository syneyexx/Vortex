---
type: module
crate: worker/train
waves: [W10, W12]
updated: 2026-10-31
---
# Training en vliegwiel

## Dag 1 (W1): loggen
Elk bericht, plan, toolcall, retrieval (gebruikt ja/nee), debatronde, Judge-oordeel, feedback (goed/fout/correctie), eindantwoord, model- + promptversie. Formaat direct bruikbaar voor SFT/DPO/layer-training.

## Lus 1 — framework (W10)
Neurale layers trainen in idle; skills bijstellen op succesrate. Zie [[modules/neurale-layers]].

## Lus 2 — model (W12)
1. **Distillatie/SFT**: beste team-antwoorden → model geeft direct wat eerst een team nodig had.
2. **DPO**: gekozen vs verworpen; label "voorkeur" apart van "feitelijk correct".
3. Uit ebooks/datasets: **gegenereerde vraag-antwoordparen + redeneervoorbeelden**, geen ruwe tekst.
4. **QLoRA** op Qwen3.5-9B (safetensors, Unsloth/PEFT) → mergen/converteren naar GGUF; adapters ook los laden/hot-swappen.
5. Alleen als PC verder vrij is (idle-scheduler).

## Poortwachter
Eval-suite + schaduwrun; slechter = automatisch terug. Versiebeheer: basismodel, datasetversie, instellingen, scores, bekende regressies. Decontaminatie: nooit trainen op eval-inhoud (ook niet via geheugen, caches, near-duplicates).

## Later
Process reward model, verifier-training, continual learning zonder catastrophic forgetting (W17).
