# CARD — `vortex-neural`

**Doel:** framework-eigen neurale layers (W11): pure-Rust ONNX-runtime, registry met versies/schaduw/promotie/rollback/hot-swap, datasets, evaluatie.
**VAULT-module:** VAULT/modules/neurale-layers.md · **Waves:** W11 (af), W13, W18

## Publieke API
| Item | Doet |
| --- | --- |
| `Net::{load, from_bytes, run, run_batch}` | ONNX (Gemm, MatMul, Add/Sub/Mul/Div, Relu, Sigmoid, Tanh, Softmax, Identity) |
| `onnx::mlp_to_onnx` | MLP → ONNX vanuit Rust (tests/hulpnetten) |
| `LayerRegistry::{open, install, promote, rollback, set_mode, record_eval, auto_decide, status}` | versies per layer; implementeert `vortex_core::LayerScorer` |
| `spec::{specs, get, LayerSpec, LayerKind}` | de 7 layers: kenmerken (volgorde = contract), klassen, waar gebruikt |
| `dataset::{write, read, evaluate, eval_index, contaminated, Example, EvalReport}` | datasets (versie, decontaminatie, split) + net vs heuristiek |
| `calibration::{Calibrator, Platt, measure}` | kalibratie (W13.06) |

## Haak
`vortex_core::LayerScorer::decide(layer, x, heuristiek, referentie)` — consumenten: cognition (router, salience), memory (importance, usefulness, entity, decay), tools (skill).

## Events (schrijft)
`neural.example.logged`, `neural.shadow.compared`, `neural.layer.{trained,promoted,rolled_back}`, `neural.dataset.exported` (server).

## Opslag
`<data>/layers/<layer>/<versie>.onnx` + `manifest.json`; datasets in `<data>/layers/datasets/`; tabellen `neural_labels`, `neural_runs` (migratie 0009).

## Tests
`cargo test -p vortex-neural` — unit (runtime, dataset) + `tests/gate.rs` (net wint → promotie + hot-swap, schaduwlog, drift → rollback, herstart).
