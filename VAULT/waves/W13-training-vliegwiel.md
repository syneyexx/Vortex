---
wave: W13
status: open
modules: [training, neurale-layers]
---
# W13 — Training en vliegwiel
Zie [[modules/training]].

- [ ] `W13.01` Dataset-builders: SFT (distillatie team-antwoorden), DPO (gekozen/verworpen, voorkeur vs correct), Q&A/redeneervoorbeelden uit ebooks/datasets
- [ ] `W13.02` QLoRA-pipeline (Unsloth/PEFT) op Qwen3.5-9B safetensors; mergen/converteren → GGUF
- [ ] `W13.03` Adapter-registry: basismodel, datasetversie, instellingen, scores, regressies
- [ ] `W13.04` Adapters los laden/hot-swappen waar backend het kan
- [ ] `W13.05` Verifier/reward model + process reward model (tussenstappen)
- [ ] `W13.06` Confidence calibrator + kalibratiemeting (gescheiden dev/test)
- [ ] `W13.07` Poortwachter: eval + schaduwrun → promoveren of terug
- [ ] `W13.08` Training alleen in idle, GPU vrijgeven bij gebruik
- [ ] `W13.09` Training/evals-pagina in GUI

## Gate
Een adapter wint op de eval zonder regressie elders; pipeline volledig herhaalbaar.
