---
wave: W10
status: open
modules: [neurale-layers, training]
---
# W10 — Neurale layers
Zie [[modules/neurale-layers]].

- [ ] `W10.01` Feature-extractie uit event log → datasets per layer (versie, decontaminatie)
- [ ] `W10.02` Bootstrap-labeling met LM
- [ ] `W10.03` Trainingsscripts (PyTorch) per layer + ONNX-export
- [ ] `W10.04` `vortex-neural`: ONNX laden via `ort` (1060/CPU), hot-swap van versies
- [ ] `W10.05` Layers: router-net, importance scorer, entity matcher, salience-net, usefulness-predictor, skill selector, decay-predictor
- [ ] `W10.06` Reranker fine-tune op eigen feedback
- [ ] `W10.07` Schaduwmodus + automatische promotie/rollback via eval
- [ ] `W10.08` Idle-training (koppeling scheduler)
- [ ] `W10.09` Dashboard: versies en scores per layer

## Gate
Elk net wint aantoonbaar van zijn heuristiek op de eval; rollback getest.
