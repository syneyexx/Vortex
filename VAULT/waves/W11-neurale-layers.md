---
wave: W11
status: af  # gate in tests (net wint → promotie, drift → rollback); reranker-training + echte layer-scores op de PC
modules: [neurale-layers, training]
---
# W11 — Neurale layers
Zie [[modules/neurale-layers]].

- [x] `W11.01` Feature-extractie uit event log → datasets per layer (versie, decontaminatie) — `LayerScorer::decide` logt elk voorbeeld (`neural.example.logged`: x, heuristiek, referentie); `build_examples` koppelt uitkomsten (gebruik, feedback, labels); `dataset::write`: versie = inhoudshash, decontaminatie (8-grammen eval-set), vaste train/test-split
- [x] `W11.02` Bootstrap-labeling met LM — `bootstrap` (router, salience, entity, skill, usefulness): LM labelt voorbeelden zonder uitkomst → `neural_labels` (bron lm/human/outcome); knop "LM-labels"
- [x] `W11.03` Trainingsscripts (PyTorch) per layer + ONNX-export — `worker/train_layer.py`: PyTorch (GPU als die er is) of numpy-trainer, gewogen verlies, early stopping, ONNX-export (Sub/Div/Gemm/Relu/Sigmoid|Softmax + metadata); gRPC `Train` (`layer:<naam>`)
- [x] `W11.04` `vortex-neural`: ONNX laden via `ort` (1060/CPU), hot-swap van versies — eigen pure-Rust ONNX-runtime (MLP-operatoren, µs; i.p.v. `ort`: geen native DLL nodig) + `LayerRegistry` met hot-swap (manifest per layer, versies)
- [x] `W11.05` Layers: router-net, importance scorer, entity matcher, salience-net, usefulness-predictor, skill selector, decay-predictor — 7 layers met vaste featurevectoren en haken: router (`route_with`), importance (geheugen schrijven), entity matcher (entiteiten samenvoegen), salience (`tag_with`), usefulness (eindscore retrieval), skill selector, decay-predictor
- [~] `W11.06` Reranker fine-tune op eigen feedback — `POST /api/neural/reranker/dataset` (vraag, passage, label uit retrieval + feedback) + `worker/scripts/train_reranker.py` (LoRA op Qwen3-Reranker, P("yes")) → **trainen zelf op de PC** (PyTorch)
- [x] `W11.07` Schaduwmodus + automatische promotie/rollback via eval — schaduwmodus (net draait mee, gelogd, overeenstemming geteld); `auto_decide`: promoveren alleen als het net ≥ 2 pt wint van heuristiek én actieve versie; actieve versie die verliest → automatisch terug
- [x] `W11.08` Idle-training (koppeling scheduler) — `spawn_idle_training`: elke 10 min bij idle (Dirigent) en ≥ 50 nieuwe voorbeelden → training als P3-run
- [x] `W11.09` Dashboard: versies en scores per layer — Dashboard → "Neurale layers": modus, versies, eval net vs heuristiek, schaduw-overeenstemming, voorbeelden, acties

## Gate
Elk net wint aantoonbaar van zijn heuristiek op de eval; rollback getest.
