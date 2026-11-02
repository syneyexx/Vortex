//! De crawler: frontier (SQLite) → per domein op tempo + robots.txt → ophalen (cache/negative
//! cache) → extraheren → Curator (dedup, kwaliteit, nieuwheid) → links scoren (Ranker) →
//! stoppen op budget of als nieuwe pagina's niets meer toevoegen.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use reqwest::Url;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use vortex_core::{EventLog, NegativeCache};
use vortex_types::{Event, EventKind, RunId, TraceId};

use crate::extract::extract;
use crate::rank::{hamming, link_vector, novelty, score_link, simhash, terms};
use crate::robots::Robots;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlConfig {
    pub max_pages: usize,
    pub max_depth: usize,
    pub max_secs: u64,
    /// Minimale tijd tussen twee verzoeken naar hetzelfde domein.
    pub domain_interval_ms: u64,
    pub user_agent: String,
    /// Koude schijf: ruwe HTML, tekst en gedownloade documenten (W10.14).
    pub raw_dir: Option<PathBuf>,
    /// Stop na zoveel pagina's achter elkaar met weinig nieuws.
    pub stale_after: usize,
    pub min_novelty: f64,
    pub max_bytes: usize,
    /// Domeinen tegelijk.
    pub concurrency: usize,
    pub cache_hours: i64,
    pub min_score: f64,
    /// Headless browser (Chromium/Chrome/Edge) voor JS-pagina's (W14.03): als de HTML vrijwel
    /// geen tekst oplevert maar wel scripts bevat → `--headless --dump-dom`.
    pub browser: Option<PathBuf>,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: 25,
            max_depth: 2,
            max_secs: 180,
            domain_interval_ms: 1500,
            user_agent: "Vortex/0.1 (persoonlijke onderzoeksassistent; respecteert robots.txt)".into(),
            raw_dir: None,
            stale_after: 6,
            min_novelty: 0.2,
            max_bytes: 4 << 20,
            concurrency: 4,
            cache_hours: 24,
            min_score: 0.15,
            browser: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub text: String,
    pub depth: usize,
    pub score: f64,
    pub content_hash: String,
    pub raw_path: Option<String>,
    pub from_cache: bool,
    pub domain: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrawlReport {
    pub pages: Vec<Page>,
    /// Gedownloade documenten (PDF e.d.) → ingest.
    pub documents: Vec<String>,
    pub fetched: usize,
    pub from_cache: usize,
    pub robots_blocked: usize,
    pub duplicates: usize,
    pub low_quality: usize,
    pub failed: usize,
    pub stopped: String,
}

#[derive(Debug)]
pub struct Crawler {
    http: reqwest::Client,
    log: EventLog,
    pub cfg: CrawlConfig,
    robots: Mutex<HashMap<String, Arc<Robots>>>,
    next_slot: Mutex<HashMap<String, Instant>>,
    negative: NegativeCache<String>,
    /// Ranker als neurale layer `crawl_rank` (W14.03); geen = heuristiek.
    pub scorer: vortex_core::Scorer,
}

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn host(u: &str) -> String {
    Url::parse(u)
        .ok()
        .and_then(|u| u.host_str().map(|h| format!("{h}{}", u.port().map(|p| format!(":{p}")).unwrap_or_default())))
        .unwrap_or_default()
}

/// (final_url, titel, hash, raw_path, text_path, fetched_at)
type CacheRow = (String, String, String, Option<String>, Option<String>, String);

enum Fetched {
    Page(Page, Vec<(String, String)>),
    Document(String),
    Blocked,
    Failed(String),
}

impl Crawler {
    pub fn new(log: EventLog, cfg: CrawlConfig) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self {
            http,
            log,
            cfg,
            robots: Mutex::new(HashMap::new()),
            next_slot: Mutex::new(HashMap::new()),
            negative: NegativeCache::new(Duration::from_secs(1800), 10_000),
            scorer: None,
        }
    }

    async fn robots_for(&self, url: &Url) -> Arc<Robots> {
        let h = host(url.as_str());
        if let Some(r) = self.robots.lock().get(&h) {
            return r.clone();
        }
        let robots_url = format!("{}://{h}/robots.txt", url.scheme());
        self.wait_turn(&h, 0.0).await;
        let r = match self.http.get(&robots_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                Robots::parse(&resp.text().await.unwrap_or_default(), &self.cfg.user_agent)
            }
            _ => Robots::default(), // geen robots.txt = alles mag
        };
        let r = Arc::new(r);
        self.robots.lock().insert(h, r.clone());
        r
    }

    /// Per-domein tempo: reserveer het volgende tijdslot en wacht tot dan.
    async fn wait_turn(&self, h: &str, crawl_delay: f64) {
        let gap =
            Duration::from_millis(self.cfg.domain_interval_ms).max(Duration::from_secs_f64(crawl_delay.min(30.0)));
        let at = {
            let mut g = self.next_slot.lock();
            let now = Instant::now();
            let at = g.get(h).copied().filter(|t| *t > now).unwrap_or(now);
            g.insert(h.to_string(), at + gap);
            at
        };
        tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
    }

    pub fn reliability(&self, domain: &str) -> f64 {
        self.log
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT reliability FROM domain_stats WHERE domain = ?1", [domain], |r| {
                    r.get::<_, f64>(0)
                })
                .optional()?)
            })
            .ok()
            .flatten()
            .unwrap_or(0.5)
    }

    /// Leren (W10.08 stap 8): bruikbaarheid en gestaafde/tegengesproken claims per domein.
    pub fn record_outcome(&self, domain: &str, useful: bool, supported: i64, contradicted: i64) {
        let now = vortex_types::now().to_rfc3339();
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "INSERT INTO domain_stats(domain, fetched, useful, supported, contradicted, updated_at) VALUES (?1, 1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(domain) DO UPDATE SET fetched = fetched + 1, useful = useful + ?2, supported = supported + ?3,
                   contradicted = contradicted + ?4, updated_at = ?5",
                params![domain, i64::from(useful), supported, contradicted, now],
            )?;
            // Betrouwbaarheid: Beta-gemiddelde over gestaafd vs tegengesproken (+ bruikbaarheid).
            tx.execute(
                "UPDATE domain_stats SET reliability = (1.0 + supported + 0.5 * useful) / (2.0 + supported + 2.0 * contradicted + 0.5 * fetched) WHERE domain = ?1",
                [domain],
            )?;
            Ok(())
        });
    }

    fn cached(&self, url: &str) -> Option<(Page, Vec<(String, String)>)> {
        let row: Option<CacheRow> = self
            .log
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT final_url, title, content_hash, raw_path, text_path, fetched_at FROM crawl_pages WHERE url = ?1 AND status < 400",
                    [url],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
                .optional()?)
            })
            .ok()
            .flatten();
        let (final_url, title, hash, raw, text_path, at) = row?;
        let age = vortex_types::now().signed_duration_since(chrono_parse(&at)?);
        if age.num_hours() >= self.cfg.cache_hours {
            return None;
        }
        let cache = std::fs::read_to_string(text_path?).ok()?;
        let (text, links) = match cache.split_once('\u{1e}') {
            Some((t, j)) => (t.to_string(), serde_json::from_str(j).unwrap_or_default()),
            None => (cache, vec![]),
        };
        Some((
            Page {
                url: url.into(),
                final_url: final_url.clone(),
                title,
                text,
                depth: 0,
                score: 0.0,
                content_hash: hash,
                raw_path: raw,
                from_cache: true,
                domain: host(&final_url),
            },
            links,
        ))
    }

    async fn fetch_one(&self, url: &str, depth: usize) -> Fetched {
        if let Some(why) = self.negative.get(&url.to_string()) {
            return Fetched::Failed(format!("eerder mislukt: {why}"));
        }
        let Ok(u) = Url::parse(url) else { return Fetched::Failed("ongeldige URL".into()) };
        let robots = self.robots_for(&u).await;
        let path = format!("{}{}", u.path(), u.query().map(|q| format!("?{q}")).unwrap_or_default());
        if !robots.allowed(&path) {
            return Fetched::Blocked;
        }
        if let Some((mut p, links)) = self.cached(url) {
            p.depth = depth;
            return Fetched::Page(p, links);
        }
        let h = host(url);
        self.wait_turn(&h, robots.crawl_delay.unwrap_or(0.0)).await;
        let resp = match self.http.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                self.negative.put(url.to_string(), e.to_string());
                return Fetched::Failed(e.to_string());
            }
        };
        let status = resp.status();
        let final_url = resp.url().to_string();
        let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        if !status.is_success() {
            self.negative.put(url.to_string(), status.to_string());
            return Fetched::Failed(format!("HTTP {status}"));
        }
        let mut bytes = Vec::new();
        let mut resp = resp;
        while let Ok(Some(chunk)) = resp.chunk().await {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > self.cfg.max_bytes {
                break;
            }
        }
        let hash = format!("{:016x}", fnv(&bytes));
        let raw_dir = self.cfg.raw_dir.clone();
        // Documenten (PDF, EPUB, DOCX) → opslaan voor ingest.
        if ct.contains("pdf") || ct.contains("epub") || ct.contains("officedocument") || final_url.ends_with(".pdf") {
            if let Some(d) = &raw_dir {
                let ext = if ct.contains("pdf") || final_url.ends_with(".pdf") {
                    "pdf"
                } else if ct.contains("epub") {
                    "epub"
                } else {
                    "docx"
                };
                let p = d.join("docs").join(format!("{hash}.{ext}"));
                if std::fs::create_dir_all(d.join("docs")).is_ok() && std::fs::write(&p, &bytes).is_ok() {
                    return Fetched::Document(p.display().to_string());
                }
            }
            return Fetched::Failed("document zonder opslagmap".into());
        }
        let html = String::from_utf8_lossy(&bytes).to_string();
        let base = Url::parse(&final_url).unwrap_or(u);
        let mut ex = if ct.contains("html") || html.trim_start().starts_with('<') {
            extract(&html, &base)
        } else {
            crate::extract::Extracted { title: String::new(), text: html.clone(), links: vec![] }
        };
        // JS-pagina: weinig tekst, wel scripts → headless browser.
        if let Some(b) = &self.cfg.browser
            && needs_browser(&html, &ex.text)
            && let Some(dom) = render(b, &final_url, 30).await
        {
            let r = extract(&dom, &base);
            if r.text.len() > ex.text.len() {
                ex = r;
            }
        }
        let (mut raw_path, mut text_path) = (None, None);
        if let Some(d) = &raw_dir {
            let rp = d.join("raw").join(format!("{hash}.html"));
            let tp = d.join("text").join(format!("{hash}.txt"));
            let _ = std::fs::create_dir_all(d.join("raw"));
            let _ = std::fs::create_dir_all(d.join("text"));
            if std::fs::write(&rp, &bytes).is_ok() {
                raw_path = Some(rp.display().to_string());
            }
            // Tekst + links (voor de cache) als JSON-voetregel.
            let cache = format!("{}\n\u{1e}{}", ex.text, serde_json::to_string(&ex.links).unwrap_or_default());
            if std::fs::write(&tp, cache).is_ok() {
                text_path = Some(tp.display().to_string());
            }
        }
        let sim = simhash(&ex.text);
        let now = vortex_types::now().to_rfc3339();
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "INSERT INTO crawl_pages(url, final_url, title, content_hash, simhash, raw_path, text_path, status, bytes, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(url) DO UPDATE SET final_url = ?2, title = ?3, content_hash = ?4, simhash = ?5, raw_path = ?6, text_path = ?7, status = ?8, bytes = ?9, fetched_at = ?10",
                params![url, final_url, ex.title, hash, format!("{sim:016x}"), raw_path, text_path, i64::from(status.as_u16()), bytes.len() as i64, now],
            )?;
            Ok(())
        });
        let page = Page {
            url: url.into(),
            final_url: final_url.clone(),
            title: ex.title,
            text: ex.text,
            depth,
            score: 0.0,
            content_hash: hash,
            raw_path,
            from_cache: false,
            domain: host(&final_url),
        };
        Fetched::Page(page, ex.links)
    }

    /// Gericht crawlen voor `query` vanaf `seeds`.
    pub async fn crawl(
        &self,
        task_id: &str,
        query: &str,
        seeds: Vec<String>,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> CrawlReport {
        let started = Instant::now();
        let q = terms(query);
        let now = vortex_types::now().to_rfc3339();
        let _ = self.log.db().write(|tx| {
            for s in &seeds {
                tx.execute(
                    "INSERT OR IGNORE INTO crawl_frontier(task_id, url, depth, score, status, added_at) VALUES (?1, ?2, 0, 1.0, 'queued', ?3)",
                    params![task_id, s, now],
                )?;
            }
            Ok(())
        });
        let mut rep = CrawlReport::default();
        let mut seen_hash: HashSet<String> = HashSet::new();
        let mut sims: Vec<u64> = Vec::new();
        let mut grams: HashSet<u64> = HashSet::new();
        let mut domains: HashSet<String> = HashSet::new();
        let mut stale = 0usize;
        loop {
            if cancel.is_cancelled() {
                rep.stopped = "geannuleerd".into();
                break;
            }
            if rep.pages.len() >= self.cfg.max_pages {
                rep.stopped = "paginabudget bereikt".into();
                break;
            }
            if started.elapsed() >= Duration::from_secs(self.cfg.max_secs) {
                rep.stopped = "tijdbudget bereikt".into();
                break;
            }
            if stale >= self.cfg.stale_after {
                rep.stopped = "nieuwe pagina's voegen niets meer toe".into();
                break;
            }
            // Beste kandidaten, hooguit één per domein per ronde.
            let batch: Vec<(String, usize, f64)> = self
                .log
                .db()
                .read(|c| {
                    let mut st = c.prepare("SELECT url, depth, score FROM crawl_frontier WHERE task_id = ?1 AND status = 'queued' ORDER BY score DESC LIMIT 50")?;
                    let v = st.query_map(params![task_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, f64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                    Ok(v)
                })
                .unwrap_or_default();
            let mut hosts = HashSet::new();
            let batch: Vec<_> =
                batch.into_iter().filter(|(u, _, _)| hosts.insert(host(u))).take(self.cfg.concurrency.max(1)).collect();
            if batch.is_empty() {
                rep.stopped = "frontier leeg".into();
                break;
            }
            let results = futures::future::join_all(batch.iter().map(|(u, d, _)| self.fetch_one(u, *d))).await;
            for ((url, depth, score), res) in batch.into_iter().zip(results) {
                let status = match res {
                    Fetched::Blocked => {
                        rep.robots_blocked += 1;
                        "skipped"
                    }
                    Fetched::Failed(why) => {
                        tracing::debug!(%url, %why, "crawl mislukt");
                        rep.failed += 1;
                        "failed"
                    }
                    Fetched::Document(p) => {
                        rep.documents.push(p);
                        "fetched"
                    }
                    Fetched::Page(mut page, links) => {
                        page.score = score;
                        if page.from_cache {
                            rep.from_cache += 1;
                        } else {
                            rep.fetched += 1;
                        }
                        // Links → frontier (Ranker), ook van pagina's die we zelf niet bewaren.
                        if depth < self.cfg.max_depth {
                            let now = vortex_types::now().to_rfc3339();
                            let scored: Vec<(String, String, f64)> = links
                                .iter()
                                .take(200)
                                .map(|(l, a)| {
                                    let d = host(l);
                                    let (seen, rel) = (domains.contains(&d), self.reliability(&d));
                                    let h = score_link(&q, l, a, depth + 1, seen, rel);
                                    let x = link_vector(&q, l, a, depth + 1, seen, rel);
                                    let s = vortex_core::neural::decide(
                                        &self.scorer,
                                        "crawl_rank",
                                        &x,
                                        vec![h as f32],
                                        &format!("{task_id}|{l}"),
                                    );
                                    (l.clone(), a.clone(), f64::from(s.first().copied().unwrap_or(h as f32)))
                                })
                                .filter(|(_, _, s)| *s >= self.cfg.min_score)
                                .collect();
                            let _ = self.log.db().write(|tx| {
                                for (l, a, s) in &scored {
                                    tx.execute(
                                        "INSERT OR IGNORE INTO crawl_frontier(task_id, url, depth, score, status, parent, anchor, added_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, ?7)",
                                        params![task_id, l, (depth + 1) as i64, s, url, a, now],
                                    )?;
                                }
                                Ok(())
                            });
                        }
                        // Curator: exact + near-duplicate, kwaliteit, nieuwheid.
                        let sim = simhash(&page.text);
                        if !seen_hash.insert(page.content_hash.clone()) || sims.iter().any(|s| hamming(*s, sim) <= 3) {
                            rep.duplicates += 1;
                        } else if page.text.chars().count() < 200 {
                            rep.low_quality += 1;
                        } else {
                            sims.push(sim);
                            let nov = novelty(&page.text, &mut grams);
                            if nov < self.cfg.min_novelty {
                                stale += 1;
                            } else {
                                stale = 0;
                            }
                            domains.insert(page.domain.clone());
                            let mut ev = Event::new(
                                EventKind::CrawlPageFetched,
                                trace,
                                serde_json::json!({"task": task_id, "url": page.final_url, "title": page.title, "depth": depth, "score": score, "novelty": nov, "cache": page.from_cache}),
                            );
                            ev.run_id = run;
                            let _ = self.log.append(ev);
                            rep.pages.push(page);
                        }
                        "fetched"
                    }
                };
                let _ = self.log.db().write(|tx| {
                    tx.execute(
                        "UPDATE crawl_frontier SET status = ?3 WHERE task_id = ?1 AND url = ?2",
                        params![task_id, url, status],
                    )?;
                    Ok(())
                });
            }
        }
        rep
    }
}

fn chrono_parse(s: &str) -> Option<vortex_types::Timestamp> {
    s.parse().ok()
}

// VX:W14.03
/// Lijkt dit een JS-pagina? (weinig zichtbare tekst, wel scripts of een lege app-root)
pub fn needs_browser(html: &str, text: &str) -> bool {
    let l = html.to_lowercase();
    text.chars().count() < 200
        && (l.matches("<script").count() >= 1 || l.contains("id=\"root\"") || l.contains("id=\"app\""))
}

/// DOM na JavaScript via een headless browser (`--headless --dump-dom`).
pub async fn render(browser: &std::path::Path, url: &str, timeout_s: u64) -> Option<String> {
    let mut cmd = tokio::process::Command::new(browser);
    cmd.args([
        "--headless",
        "--disable-gpu",
        "--no-sandbox",
        "--no-first-run",
        "--disable-extensions",
        "--virtual-time-budget=8000",
        "--dump-dom",
        url,
    ])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(timeout_s), cmd.output()).await.ok()?.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string()).filter(|s| !s.trim().is_empty())
}

/// Zoek een headless-geschikte browser op de gangbare plekken.
pub fn find_browser() -> Option<PathBuf> {
    let cands = [
        "/opt/pw-browsers/chromium",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    cands.iter().map(PathBuf::from).find(|p| p.is_file())
}
