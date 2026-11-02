# CARD — `vortex-crawler`

**Doel:** Webcrawler-team (W10.07): ophalen, extraheren, ranken, cureren; frontier in SQLite.
**VAULT-module:** VAULT/modules/research-kennis.md · **Waves:** W10 (af), W14 (af)

## Publieke API
| Item | Doet |
| --- | --- |
| `Crawler::new(log, CrawlConfig)` | config: budgetten (pagina's, diepte, tijd), tempo per domein, raw_dir (koude schijf), stale_after/min_novelty, cache_hours |
| `Crawler::crawl(task, query, seeds, run, trace, cancel) -> CrawlReport` | focused crawl; pagina's + documenten (PDF) + statistiek + stopreden |
| `Crawler::{reliability, record_outcome}` | bronbetrouwbaarheid per domein (leert van research) |
| `extract::extract(html, base)` | titel, hoofdtekst, links met ankertekst |
| `robots::Robots` | robots.txt (groepen, langste match, `*`/`$`, crawl-delay) |
| `rank::{score_link, link_vector, simhash, novelty}` | Ranker (heuristiek + kenmerken voor layer `crawl_rank`) + Curator-hulpen |
| `Crawler.scorer` | neurale ranker (`crawl_rank`) via `vortex_core::Scorer`; geen = heuristiek |
| `crawl::{needs_browser, render, find_browser}` + `CrawlConfig.browser` | headless browser (`--dump-dom`) voor JS-pagina's |
| `world::WorldMonitor::{new, due, fetch, store, follow, unfollow, followed, recrawl_due, link_entities, source_bias}` | wereldmonitor: tempo per soort, inhaal-modus, opslag, gevolgde sites, Wikidata, bronbias |
| `world::sources::{SourceSpec, Item, fetch, parse_feed, norm_date, wikidata_lookup}` + `world::terms` | connectors (12 soorten) + voorwaarden |
| `world::cluster::{cluster, tokens, jaccard, theme}` | event-clustering (gemeld vs bevestigd), thema |

## Tabellen
`crawl_frontier`, `crawl_pages`, `domain_stats` (migratie 0008); `world_sources`, `world_items`, `world_events`, `followed_sites` (migratie 0012).

## Events (schrijft)
`crawl.page.fetched`, `world.source.fetched`, `world.event.updated`.

## Invarianten
- robots.txt altijd gerespecteerd; minimaal `domain_interval_ms` tussen verzoeken per domein (of crawl-delay).
- Ruwe HTML/tekst/documenten alleen op `raw_dir` (koude schijf).

## Tests
`cargo test -p vortex-crawler` — unit (extract, robots, rank, feeds, namen, clustering) + `tests/crawl.rs` (lokale site: robots, volgorde, dedup, tempo, frontier, cache, PDF, budget; JS-pagina via echte headless Chromium) + `tests/world.rs` (RSS/GDELT/USGS/HN/World Bank tegen nepserver, dedup, interval, clustering bevestigd/gemeld, Wikidata, bias).
