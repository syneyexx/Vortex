---
type: module
crate: vortex-memory
waves: [W05, W12]
updated: 2026-10-31
---
# Geheugen

## Typen
| Type | Inhoud | Wave |
| --- | --- | --- |
| Working | = Global Workspace ([[modules/cognitie]]) | W07 |
| Short-term | recente berichten/toolresultaten sessie | W05 |
| Long-term | alles wat schrijfpolicy goedkeurt | W05 |
| Episodic | gebeurtenissen/sessies met tijd | W05 |
| Semantic | feiten, begrippen, entiteiten, relaties = temporal knowledge graph | W05 |
| User/autobiografisch + project | voorkeuren, doelen, projectkennis (werkruimtes) | W05 |
| World/domain | wereldmonitor, sectoren | W14 |
| Procedural | skills, werkwijzen, succesrate | W12 |
| Meta-memory | wat Vortex weet, hoe zeker, gaten | W12 |

## Opslag
SQLite = waarheid (herinneringen, KV, tijd, FTS5, edges-tabel, validity from/to); LanceDB = vectoren (int8, evt. 512 dims, multimodaal); moka-cache. Per vector: embeddingmodel, versie, dimensie, normalisatie, indexgeneratie. Index-generaties: nieuw bouwen → omschakelen. Content-hashes. WAL-checkpoints. Migraties. Zie [[04-HARDWARE-EN-OPSLAG]].

## Schrijven
1. Schrijfpolicy (importance scorer) — wanneer opslaan
2. Structuur-bewust chunken + **contextual chunk description** (origineel apart)
3. Embeddings (meerdere modellen mogelijk)
4. Entity extraction & linking → graph; claims extraheren
5. Salience/emotie-tag, confidence, bron (source tracking), tijd
6. Dedup / merge (entity matcher)
7. Conflict detection → [[modules/redeneren-debat]] conflict-team
8. Automatische samenvatting; herkomstafhankelijkheden registreren
9. Tool results automatisch in geheugen (via policy)
10. Multimodaal: tekst, code, afbeeldingen, bestanden (verwijzingen)

## Ophalen
Query rewriting → parallel vector + keyword (FTS5/BM25) + graph traversal/multi-hop + temporeel → samenvoegen → rerank → MMR-diversiteit + gereserveerd tegenbewijs → ranking relevantie + recency + importance (+ usefulness, W11) → filters (type, tijd, bron, vertrouwen, werkruimte) → contextueel (huidige taak) → proactief ophalen → herkomst van scores zichtbaar → Context Inspector.

## Onderhoud ("dreaming", idle)
Consolidatie short→long, decay/forgetting, pruning, clustering/merging, hiërarchische samenvattingen (dag/week/maand en sectie/hoofdstuk/boek), compressie naar HDD (tiering), periodieke reflectie, contradiction resolution, confidence-updates, global corpus synthesis.

## Controle
Bekijken, bewerken, verwijderen met **deletion propagation** (alle indexen, samenvattingen, caches), uitzetten/beperken, granulaire scoping per werkruimte, isolatie tussen projecten, export/import, backups → HDD, versioning per herinnering.

## Schaal 3–4 TB
Index + samenvattingen op SSD, inhoud zstd op HDD in sequentiële blokken, ophalen → SSD-cache, prefetch, tiering via decay-predictor.

## Metrics
Recall@k, rang, irrelevante passages, vragen zonder antwoord, gebruik per herinnering, coverage/gaps.
