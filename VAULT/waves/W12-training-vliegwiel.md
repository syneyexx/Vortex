---
wave: W12
status: open
modules: [training, neurale-layers]
---
# W12 — Training en vliegwiel
Zie [[modules/training]].

- [ ] `W12.01` Dataset-builders: SFT (distillatie team-antwoorden), DPO (gekozen/verworpen, voorkeur vs correct), Q&A/redeneervoorbeelden uit ebooks/datasets
- [ ] `W12.02` QLoRA-pipeline (Unsloth/PEFT) op Qwen3.5-9B safetensors; mergen/converteren → GGUF
- [ ] `W12.03` Adapter-registry: basismodel, datasetversie, instellingen, scores, regressies
- [ ] `W12.04` Adapters los laden/hot-swappen waar backend het kan
- [ ] `W12.05` Verifier/reward model + process reward model (tussenstappen)
- [ ] `W12.06` Confidence calibrator + kalibratiemeting (gescheiden dev/test)
- [ ] `W12.07` Poortwachter: eval + schaduwrun → promoveren of terug
- [ ] `W12.08` Training alleen in idle, GPU vrijgeven bij gebruik
- [ ] `W12.09` Training/evals-pagina in GUI

## Gate
Een adapter wint op de eval zonder regressie elders; pipeline volledig herhaalbaar.
