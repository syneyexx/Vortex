---
wave: W13
status: af  # pijplijn + gate-logica getest; echte QLoRA-training en adapter-eval op de PC (PyTorch + GPU)
modules: [training, neurale-layers]
---
# W13 — Training en vliegwiel
Zie [[modules/training]].

- [x] `W13.01` Dataset-builders: SFT (distillatie team-antwoorden), DPO (gekozen/verworpen, voorkeur vs correct), Q&A/redeneervoorbeelden uit ebooks/datasets — `build_sft` (Judge-goedgekeurde team-antwoorden zonder negatieve feedback, 👍-antwoorden, correcties), `build_dpo` (gekozen vs laagst gescoorde kandidaat; label correctness/judge vs preference), `build_qa` (vraag-antwoord + redenering uit ingelezen secties, met locator); dedup + decontaminatie (8-grammen eval) + versie
- [~] `W13.02` QLoRA-pipeline (Unsloth/PEFT) op Qwen3.5-9B safetensors; mergen/converteren → GGUF — `worker/train_lora.py`: Unsloth of PEFT (+bitsandbytes 4-bit) + TRL SFT/DPO, checkpoints, adapter → GGUF (`convert_lora_to_gguf.py`), optioneel mergen → GGUF; `validate()` droogloop; gRPC `lora:sft|dpo` → **echte training op de PC** (PyTorch)
- [x] `W13.03` Adapter-registry: basismodel, datasetversie, instellingen, scores, regressies — tabel `adapters` (basismodel, dataset, instellingen, scores per suite, regressies, schaduw, checkpoint, metrics, status) + `training_datasets` (migratie 0011)
- [x] `W13.04` Adapters los laden/hot-swappen waar backend het kan — `--lora … --lora-init-without-apply` bij start; `OpenAiBackend::{lora_adapters, set_lora}` (llama-server `/lora-adapters`) + LoRA per verzoek; knop aan/uit
- [x] `W13.05` Verifier/reward model + process reward model (tussenstappen) — layers `verifier` (antwoordniveau: citaties, onzekerheid, Judge, critic, claims, objectieve check) in het debat en `prm` (stapniveau) op planstappen; kandidaten vastgelegd (`agent.candidates.recorded`); LM-rewardmodel = DPO-adapter (W13.02)
- [x] `W13.06` Confidence calibrator + kalibratiemeting (gescheiden dev/test) — `Calibrator` (Platt-schaling, ECE/Brier/nauwkeurigheid; dev/test gescheiden) op Judge-zekerheid vs feedback; toegepast in de Judge-gate
- [x] `W13.07` Poortwachter: eval + schaduwrun → promoveren of terug — `gate`: adapter aan → alle suites met basislijn → geen regressie > 2 pt én ergens ≥ 2 pt winst én schaduwrun (blind pairwise Judge op recente vragen) ≥ 50 % → actief, anders afgewezen + adapter uit
- [x] `W13.08` Training alleen in idle, GPU vrijgeven bij gebruik — training als P3-run met GPU-lease; gebruiker actief of preemptie → worker annuleren, checkpoint, status gepauzeerd; vliegwiel hervat/start in idle (≥ min_new_examples)
- [x] `W13.09` Training/evals-pagina in GUI — Modellen → "Training & evals": datasets bouwen, trainen, adapters met scores/regressies/schaduw, poortwachter, aan/uit, kalibratie

## Gate
Een adapter wint op de eval zonder regressie elders; pipeline volledig herhaalbaar.
