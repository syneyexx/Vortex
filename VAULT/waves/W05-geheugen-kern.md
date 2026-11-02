---
wave: W05
status: bezig  # gate (recall/vergeten/backup) groen met hash-embedder; met echt model op PC
modules: [geheugen]
---
# W05 — Geheugen kern
Zie [[modules/geheugen]].

- [x] `W05.01` `MemoryStore`-trait; SQLite-schema (herinneringen, typen, bron, tijd, validity, confidence, salience, versie, werkruimte, edges, FTS5)
- [~] `W05.02` LanceDB-index (int8, dims configureerbaar) + embedding-metadata per vector + index-generaties — **afwijking:** eigen int8-index (`FlatI8Index`, in SQLite, zelfde transactie) i.p.v. LanceDB achter `VectorIndex`-trait; generaties + metadata af — zie [[OPEN-VRAGEN]]
- [~] `W05.03` Embedding-service (1060) met single-flight en batching — batching + single-flight + MRL-inkorting af en getest; **echte Qwen3-Embedding op de 1060 verifiëren op de PC**
- [x] `W05.04` Schrijfpipeline: policy (heuristisch), chunking, contextbeschrijving, entiteiten/claims (router-model), graph-koppeling, salience, bron, samenvatting, herkomstafhankelijkheden
- [x] `W05.05` Dedup/merge (similarity-drempel) + conflictdetectie → conflictregister (team volgt in W09)
- [x] `W05.06` Short-term, long-term, episodic, semantic, user/project-geheugen
- [x] `W05.07` Ophalen: query rewriting, vector + FTS5 + graph (multi-hop) + temporeel, samenvoegen, rerank (Qwen3-Reranker), MMR, tegenbewijs-slot, ranking relevantie+recency+importance, filters, scoreherkomst — rerank via Qwen3-Reranker zodra de 1060-server draait (adapter af)
- [x] `W05.08` Tool results → geheugen via policy
- [x] `W05.09` Deletion propagation (alle indexen, samenvattingen, caches) + "vergeet dit"
- [x] `W05.10` Bron gewijzigd → afgeleiden verouderd
- [x] `W05.11` Project-werkruimtes (isolatie + gedeelde algemene kennis), cache-isolatie
- [x] `W05.12` Export/import, backup → HDD, restore getest — restore getest; backup standaard naar `paths.cold_dir`
- [x] `W05.13` Memory-pagina in GUI: bekijken, bewerken, vergeten, filters, conflictregister, Retrieved memory-paneel live
- [x] `W05.14` Basis decay + samenvattingen

## Gate
Recall-tests (retrieval eval-set, start) groen; vergeten verwijdert aantoonbaar overal; backup/restore werkt.
