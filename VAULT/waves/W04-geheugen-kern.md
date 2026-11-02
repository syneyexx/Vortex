---
wave: W04
status: open
modules: [geheugen]
---
# W04 — Geheugen kern
Zie [[modules/geheugen]].

- [ ] `W04.01` `MemoryStore`-trait; SQLite-schema (herinneringen, typen, bron, tijd, validity, confidence, salience, versie, werkruimte, edges, FTS5)
- [ ] `W04.02` LanceDB-index (int8, dims configureerbaar) + embedding-metadata per vector + index-generaties
- [ ] `W04.03` Embedding-service (1060) met single-flight en batching
- [ ] `W04.04` Schrijfpipeline: policy (heuristisch), chunking, contextbeschrijving, entiteiten/claims (router-model), graph-koppeling, salience, bron, samenvatting, herkomstafhankelijkheden
- [ ] `W04.05` Dedup/merge (similarity-drempel) + conflictdetectie → conflictregister (team volgt in W08)
- [ ] `W04.06` Short-term, long-term, episodic, semantic, user/project-geheugen
- [ ] `W04.07` Ophalen: query rewriting, vector + FTS5 + graph (multi-hop) + temporeel, samenvoegen, rerank (Qwen3-Reranker), MMR, tegenbewijs-slot, ranking relevantie+recency+importance, filters, scoreherkomst
- [ ] `W04.08` Tool results → geheugen via policy
- [ ] `W04.09` Deletion propagation (alle indexen, samenvattingen, caches) + "vergeet dit"
- [ ] `W04.10` Bron gewijzigd → afgeleiden verouderd
- [ ] `W04.11` Project-werkruimtes (isolatie + gedeelde algemene kennis), cache-isolatie
- [ ] `W04.12` Export/import, backup → HDD, restore getest
- [ ] `W04.13` Memory-pagina in GUI: bekijken, bewerken, vergeten, filters, conflictregister, Retrieved memory-paneel live
- [ ] `W04.14` Basis decay + samenvattingen

## Gate
Recall-tests (retrieval eval-set, start) groen; vergeten verwijdert aantoonbaar overal; backup/restore werkt.
