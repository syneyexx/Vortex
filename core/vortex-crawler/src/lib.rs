//! Webcrawler-team (W10.07): Crawler (HTTP, links volgen), Reader/Extractor (hoofdtekst, titel,
//! links), Ranker (focused crawling: relevantie + value-of-information), Curator (exact +
//! near-duplicate dedup, kwaliteit, bronbetrouwbaarheid). Frontier per taak in SQLite; ruwe HTML
//! en tekst op de koude schijf (W10.14); per-domein tempo + robots.txt; cache + negative cache.
//! De Scout (zoekvragen + startpunten) zit in het research-team (`vortex-agents::research`).
// VX:W10.07
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod crawl;
pub mod extract;
pub mod rank;
pub mod robots;

pub use crawl::{CrawlConfig, CrawlReport, Crawler, Page};
pub use extract::{Extracted, extract};
pub mod world;
