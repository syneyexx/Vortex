# CARD — `worker/` (Python)

**Doel:** ML/data-werk: ingest + parsers (W10), training (W13), datasets (W14). Praat via gRPC met de kern.
**Contract:** `proto/vortex/worker/v1/worker.proto` · **Waves:** W00 (skelet), W10, W13, W14

## Publieke API
`WorkerService` (gRPC): `GetHealth`, `Cancel` (W00) · `Parse`, `Ingest` (W10, af) · `Train`: `layer:<naam>` (W11, af: `train_layer.py`, MLP → ONNX); LoRA/DPO volgen in W13.
`vortex_worker.parsers.parse(path)` → secties met locator; `vortex_worker.ingest.ingest(paths, out_dir)` → JSONL per document (koude schijf).

## Ingangen
`src/vortex_worker/server.py` · `parsers.py` · `ingest.py` · `scripts/gen_proto.py` (codegen → `src/vortex_worker/gen/`, niet bewerken)

## Commando's
`uv sync --extra ingest --extra layers` · reranker: `scripts/train_reranker.py` (extra `train`) · `uv run vortex-worker --address 127.0.0.1:50061` · `uv run pytest` · `uv run ruff check .`
