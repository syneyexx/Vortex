# CARD — `vortex-crawler`

**Doel:** Webcrawler-team (W10.07): ophalen, extraheren, ranken, cureren; frontier in SQLite.
**VAULT-module:** VAULT/modules/research-kennis.md · **Waves:** W10 (af), W14

## Publieke API
| Item | Doet |
| --- | --- |
| `Crawler::new(log, CrawlConfig)` | config: budgetten (pagina's, diepte, tijd), tempo per domein, raw_dir (koude schijf), stale_after/min_novelty, cache_hours |
| `Crawler::crawl(task, query, seeds, run, trace, cancel) -> CrawlReport` | focused crawl; pagina's + documenten (PDF) + statistiek + stopreden |
| `Crawler::{reliability, record_outcome}` | bronbetrouwbaarheid per domein (leert van research) |
| `extract::extract(html, base)` | titel, hoofdtekst, links met ankertekst |
| `robots::Robots` | robots.txt (groepen, langste match, `*`/`$`, crawl-delay) |
| `rank::{score_link, simhash, novelty}` | Ranker + Curator-hulpen |

## Tabellen
`crawl_frontier`, `crawl_pages`, `domain_stats` (migratie 0008).

## Invarianten
- robots.txt altijd gerespecteerd; minimaal `domain_interval_ms` tussen verzoeken per domein (of crawl-delay).
- Ruwe HTML/tekst/documenten alleen op `raw_dir` (koude schijf).

## Tests
`cargo test -p vortex-crawler` — unit (extract, robots, rank) + `tests/crawl.rs` (lokale site: robots, volgorde, dedup, tempo, frontier, cache, PDF, budget).
