---
wave: W10
status: af  # gate 1 groen (EPUB via worker → "hoofdstuk 2 > Jupiter"); research-eval met echt model + SearXNG op de PC
modules: [research-kennis]
---
# W10 — Research en ingest
Zie [[modules/research-kennis]].

- [x] `W10.01` Python-worker ingest-service (gRPC), voortgang-events — `worker`: gRPC `Parse` + `Ingest` (stream: progress, chunk_batch, error, done; annuleren per trace); kern: `ChatEngine::ingest` als run (P2/P3) met events `ingest.job.*`; zonder worker Rust-fallback (Markdown/tekst/HTML/code)
- [x] `W10.02` Parsers: EPUB, PDF (tekst), DOCX, Markdown, HTML, code — `vortex_worker.parsers`: EPUB (zip+OPF, locator "hoofdstuk N > kop"), PDF (PyMuPDF, koppen op lettergrootte, "p. N"), DOCX (zelf, Heading/Kop-stijlen), Markdown ("regel N > kop"), HTML, code (Python-ast / patronen, "regels a-b"), tekst
- [x] `W10.03` Structuur-bewust chunken, contextual chunk descriptions, content-hashes, incrementeel — secties → geheugen-schrijfpijplijn (structuur-chunken, contextbeschrijving, embeddings); content-hash per document → ongewijzigd = overslaan, gewijzigd = oude versie verouderd
- [x] `W10.04` Entiteit/claim-extractie op 1060, graph-koppeling, dedup, conflictcheck — via de W05-pijplijn op het extract-model (1060): entiteiten/claims → graph, dedup, conflictcheck
- [x] `W10.05` Hiërarchische samenvattingen (sectie → hoofdstuk → boek) — hoofdstuk- en boeksamenvattingen (Summary, derived_from, locator)
- [x] `W10.06` Onderwerp-research: plan, deelvragen, zoeken, lezen, iteratieve retrieval, adaptief bijsturen, ontbrekend bewijs zoeken, kruiscontrole — `research::run`: plan → deelvragen → zoeken (geheugen + web) → lezen → records → analyse → trianguleren → Judge/Skepticus (volgende gerichte zoekactie) → rondes
- [x] `W10.07` Webcrawler-team: Scout, Crawler (HTTP), Reader/Extractor, Ranker, Curator; focused crawling met budgetten; frontier in SQLite; ruwe HTML op HDD; per-domein tempo + robots.txt; cache — `vortex-crawler`: Crawler (HTTP, links), Reader/Extractor, Ranker (relevantie + VOI), Curator (exact + simhash, kwaliteit, nieuwheid → stoppen), frontier (`crawl_frontier`), ruwe HTML/tekst/PDF op de koude schijf, robots.txt + crawl-delay + tempo per domein, cache + negative cache; Scout in het research-team
- [x] `W10.08` Research-team volgens [[modules/research-team]]: alle 11 rollen, loop (8 stappen), drie modi, Judge-gate per deelvraag — Lead, Scouts+crawler, Readers, Extractor, Bronbeoordelaar (domeinbetrouwbaarheid + onafhankelijke groepen), Analysts, Fact-checker (trianguleren), Skepticus, Judge per deelvraag, Writer, Archivist; leren via `domain_stats`; modi quick/standaard/deep (`choose_mode`; Research-knop = deep)
- [x] `W10.09` Extractor-schema's (claim, getal, entiteit, relatie, event, tabel, citaat, bron) + validatie + direct naar geheugen via Archivist — `records`: claim, number, entity, relation, event, table, quote, source met JSON-schema + coercie; ongeldige verworpen; Archivist → `research_records` + geheugen (herkomst + locator)
- [x] `W10.10` Parallel: crawlers (CPU) + readers/extractor (1060) terwijl hoofdmodel analyseert — per ronde alle deelvragen tegelijk zoeken (CPU/netwerk) en alle bronnen tegelijk lezen/extraheren (1060); hoofdmodel analyseert daarna
- [x] `W10.11` Research-pagina: live records, deelvragen, bronnen, bewijsoverzicht, export CSV/Parquet — Research-pagina: starten, deelvragen, bronnen + betrouwbaarheid, records live, rapport, export CSV/JSONL/Parquet (pyarrow)
- [x] `W10.12` Brongebonden antwoordopbouw (passage ↔ claim), rapporten/tabellen als artefact — Analyst/Writer met [n]-verwijzingen naar genummerde bronnen (URL of document + locator), getallentabel, bronnenlijst; rapport als `.md`-artefact in data/reports
- [x] `W10.13` Global corpus synthesis — `corpus_synthesis`: map over documentsamenvattingen (1060) → reduce (hoofdmodel) met [Dn]; knop "Hele verzameling"
- [x] `W10.14` Ruwe data op HDD, kennis op SSD — `paths.cold_dir`: crawl (raw/text/docs) en ingest-JSONL; kennis (SQLite/index/rapporten) in data_dir

## Gate
Een ebook is doorzoekbaar met exacte bronverwijzing; research-eval groen.
