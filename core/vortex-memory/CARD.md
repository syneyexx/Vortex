# CARD — `vortex-memory`

**Doel:** het geheugen: schrijfpipeline, hybride ophalen, vergeten met doorwerking, conflicten, werkruimtes, backup.
**VAULT-module:** VAULT/modules/geheugen.md · **Waves:** W05 (kern), W12 (gevorderd)

## Publieke API
| Item | Doet |
| --- | --- |
| `MemoryStore::open(log, embedder, llm, MemConfig)` | opent op de kern-DB, laadt actieve indexgeneratie in RAM |
| `write(NewMemory) -> WriteOutcome` | policy → chunken (koppen/code) → contextbeschrijving → embedden → dedup/merge → conflictcheck → entiteiten/claims/graph → samenvatting → één transactie |
| `write_or_defer` / `retry_pending` | embeddingmodel weg → persistente wachtrij |
| `remember_tool_result` | toolresultaat via de policy (W05.08) |
| `retrieve(&Query, reranker)` | herschrijven → vector + BM25 + graph (2 hops) + temporeel → RRF → rerank → recency/belang → MMR → tegenbewijs-slot; `ScoreParts` = waarom |
| `get` / `list(filters, fts)` / `update` (versies) / `versions` | bekijken en bewerken |
| `forget(id)` | vergeten + afgeleiden + alle indexen + wees-entiteiten + caches |
| `upsert_source` / `mark_derived_stale` | bron gewijzigd → afgeleiden verouderd |
| `conflicts` / `resolve_conflict` | conflictregister (W09 conflict-team gebruikt dit) |
| `workspaces` | isolatie per project; algemene kennis (zonder werkruimte) altijd zichtbaar |
| `export` / `import` (JSONL) / `backup(dest)` / `MemoryStore::restore` | |
| `decay` / `summarize_session` | basis-onderhoud |
| Traits: `Embedder`, `MemLlm`, `Reranker`; impl.: `HashEmbedder`, `BatchingEmbedder`, `HeuristicLlm` | modelkoppeling zit in `vortex-context::adapters` |
| `vector::{VectorIndex, FlatI8Index}` | int8, exact; LanceDB/ANN later achter dezelfde trait |

## Opslag
Schema `vortex-core/migrations/0003_memory.sql`: memories (+FTS5 met triggers), memory_versions, vectors (int8 per generatie), index_generations, entities, memory_entities, edges, claims, derivations, conflicts, sources, workspaces, memory_pending.

## Events
`memory.item.{written,merged,updated,forgotten,staled}`, `memory.conflict.{detected,resolved}`, `memory.retrieval.performed` (met `run_id` + scoreherkomst), `decision.made` (schrijfpolicy). **Nooit inhoud in events.**

## Invarianten
- Herinnering + vector + graph + herkomst in één transactie; RAM-index pas na commit.
- Vergeten laat niets achter in DB-indexen (bewezen in `gate_forget_removes_everywhere`).
- Andere werkruimte nooit zichtbaar.

## Tests
`cargo test -p vortex-memory` — unit (13) + `tests/memory.rs` (12): W05-gate recall@k, vergeten overal, backup/restore, dedup, conflicten + tegenbewijs, verouderen, werkruimtes, export/import, chunks, decay, wachtrij, toolresultaten.
