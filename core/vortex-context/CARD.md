# CARD — `vortex-context`

**Doel:** context assembly (W07) en de koppeling geheugen ⇄ modellen.
**VAULT-module:** VAULT/modules/context.md · **Waves:** W05 (adapters), W07

## Publieke API
| Item | Doet |
| --- | --- |
| `adapters::ModelEmbedder` | `Embedder` via een backend (Qwen3-Embedding op de 1060) |
| `adapters::ModelReranker` | `Reranker` via `/v1/rerank` |
| `adapters::ModelMemLlm` | contextbeschrijving, extractie (JSON-schema), samenvatten, query-varianten via het router-model; valt terug op `HeuristicLlm` |
