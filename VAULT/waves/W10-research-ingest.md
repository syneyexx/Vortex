---
wave: W10
status: open
modules: [research-kennis]
---
# W10 — Research en ingest
Zie [[modules/research-kennis]].

- [ ] `W10.01` Python-worker ingest-service (gRPC), voortgang-events
- [ ] `W10.02` Parsers: EPUB, PDF (tekst), DOCX, Markdown, HTML, code
- [ ] `W10.03` Structuur-bewust chunken, contextual chunk descriptions, content-hashes, incrementeel
- [ ] `W10.04` Entiteit/claim-extractie op 1060, graph-koppeling, dedup, conflictcheck
- [ ] `W10.05` Hiërarchische samenvattingen (sectie → hoofdstuk → boek)
- [ ] `W10.06` Onderwerp-research: plan, deelvragen, zoeken, lezen, iteratieve retrieval, adaptief bijsturen, ontbrekend bewijs zoeken, kruiscontrole
- [ ] `W10.07` Webcrawler-team: Scout, Crawler (HTTP), Reader/Extractor, Ranker, Curator; focused crawling met budgetten; frontier in SQLite; ruwe HTML op HDD; per-domein tempo + robots.txt; cache
- [ ] `W10.08` Research-team volgens [[modules/research-team]]: alle 11 rollen, loop (8 stappen), drie modi, Judge-gate per deelvraag
- [ ] `W10.09` Extractor-schema's (claim, getal, entiteit, relatie, event, tabel, citaat, bron) + validatie + direct naar geheugen via Archivist
- [ ] `W10.10` Parallel: crawlers (CPU) + readers/extractor (1060) terwijl hoofdmodel analyseert
- [ ] `W10.11` Research-pagina: live records, deelvragen, bronnen, bewijsoverzicht, export CSV/Parquet
- [ ] `W10.12` Brongebonden antwoordopbouw (passage ↔ claim), rapporten/tabellen als artefact
- [ ] `W10.13` Global corpus synthesis
- [ ] `W10.14` Ruwe data op HDD, kennis op SSD

## Gate
Een ebook is doorzoekbaar met exacte bronverwijzing; research-eval groen.
