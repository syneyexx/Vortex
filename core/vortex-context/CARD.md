# CARD — `vortex-context`

**Doel:** context assembly (W07) en de koppeling geheugen ⇄ modellen.
**VAULT-module:** VAULT/modules/context.md · **Waves:** W05 (adapters), W07 (assembler, af)

## Publieke API
| Item | Doet |
| --- | --- |
| `adapters::ModelEmbedder` | `Embedder` via een backend (Qwen3-Embedding op de 1060) |
| `adapters::ModelReranker` | `Reranker` via `/v1/rerank` |
| `adapters::ModelMemLlm` | contextbeschrijving, extractie (JSON-schema), samenvatten, query-varianten via het router-model; valt terug op `HeuristicLlm` |
| `assembler::assemble(&AssemblyInput, &Budget) -> Assembled` | prompt per sectie binnen budget: stabiele prefix (system, rol, tools), profiel, herinneringen (+ tegenbewijs), compactie oudere beurten (exacte gegevens blijven, gists, sessiesamenvatting), sturing, vraag; `report` voor de Inspector |
| `assembler::assemble_for_role` | geïsoleerde context voor één rol/stap |
| `assembler::{exact_facts, gist, filter_tool_output, tokens}` | hulpen voor verliesbewuste compactie |
