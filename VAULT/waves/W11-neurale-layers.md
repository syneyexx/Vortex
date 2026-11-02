---
wave: W11
status: open
modules: [neurale-layers, training]
---
# W11 — Neurale layers
Zie [[modules/neurale-layers]].

- [ ] `W11.01` Feature-extractie uit event log → datasets per layer (versie, decontaminatie)
- [ ] `W11.02` Bootstrap-labeling met LM
- [ ] `W11.03` Trainingsscripts (PyTorch) per layer + ONNX-export
- [ ] `W11.04` `vortex-neural`: ONNX laden via `ort` (1060/CPU), hot-swap van versies
- [ ] `W11.05` Layers: router-net, importance scorer, entity matcher, salience-net, usefulness-predictor, skill selector, decay-predictor
- [ ] `W11.06` Reranker fine-tune op eigen feedback
- [ ] `W11.07` Schaduwmodus + automatische promotie/rollback via eval
- [ ] `W11.08` Idle-training (koppeling scheduler)
- [ ] `W11.09` Dashboard: versies en scores per layer

## Gate
Elk net wint aantoonbaar van zijn heuristiek op de eval; rollback getest.
