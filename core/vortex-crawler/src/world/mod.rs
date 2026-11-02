//! Wereldmonitor (W14): connectors (GDELT, RSS/Atom, Wikipedia Current Events, Wikidata,
//! ReliefWeb, USGS, NASA EONET, World Bank, Eurostat, CBS, arXiv, Hacker News) met tempo en
//! voorwaarden per bron, gepland ophalen met inhaal-modus, opslag (`world_items`),
//! event-clustering (artikelen → één event; gemeld vs bevestigd) en bronbias.
// VX:W14.01

pub mod cluster;
pub mod sources;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, TraceId};

pub use cluster::{ClusterReport, cluster};
pub use sources::{Item, SourceSpec};

/// Voorwaarden en tempo per soort bron (documentatie + minimale tijd tussen verzoeken).
pub fn terms(kind: &str) -> (&'static str, u64) {
    match kind {
        "gdelt" => ("GDELT DOC 2.0: gratis, geen sleutel; wees zuinig (max ~1 verzoek/5 s).", 5),
        "rss" => ("RSS/Atom van de uitgever: koppen + link, niet de volledige artikelen herpubliceren.", 1),
        "wikipedia_events" => {
            ("Wikipedia REST: CC BY-SA; User-Agent met contact verplicht; ruim binnen 200 verzoeken/s.", 1)
        }
        "wikidata" => ("Wikidata: CC0; User-Agent verplicht; terughoudend met SPARQL.", 1),
        "reliefweb" => ("ReliefWeb API: appname verplicht, max 1000 verzoeken/dag.", 10),
        "usgs" => ("USGS-feeds: publiek domein, geen sleutel.", 1),
        "eonet" => ("NASA EONET: publiek, geen sleutel.", 1),
        "worldbank" => ("World Bank API: CC BY 4.0, geen sleutel.", 1),
        "eurostat" => ("Eurostat API: hergebruik toegestaan met bronvermelding.", 2),
        "cbs" => ("CBS Open Data (OData): CC BY 4.0, bronvermelding CBS.", 1),
        "arxiv" => ("arXiv API: max 1 verzoek per 3 s, één verbinding.", 3),
        "hackernews" => ("Hacker News Firebase API: open, geen sleutel.", 1),
        _ => ("onbekende bron", 2),
    }
}

/// Kandidaat-namen: reeksen woorden met hoofdletter (niet alleen het eerste woord van de zin).
pub fn names(title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let words: Vec<&str> =
        title.split(|c: char| c.is_whitespace() || ",;:()\"'".contains(c)).filter(|w| !w.is_empty()).collect();
    for (i, w) in words.iter().enumerate() {
        let cap = w.chars().next().is_some_and(char::is_uppercase)
            && w.chars().count() > 1
            && !(i == 0 && words.get(1).is_some_and(|n| !n.chars().next().is_some_and(char::is_uppercase)));
        if cap {
            cur.push(w.trim_end_matches(['.', '!', '?']));
        } else if !cur.is_empty() {
            out.push(cur.join(" "));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out.dedup();
    out.into_iter().filter(|n| n.chars().count() >= 3).collect()
}

/// Gepland ophalen met inhaal-modus: elke bron heeft een interval; bij opstarten na een pauze
/// wordt het gemiste venster opgehaald (`since` = laatste geslaagde ophaalactie).
#[derive(Debug)]
pub struct WorldMonitor {
    pub http: reqwest::Client,
    log: EventLog,
    last_hit: Mutex<HashMap<String, Instant>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FetchReport {
    pub source: String,
    pub items: usize,
    pub new: usize,
    pub error: Option<String>,
    pub catch_up_hours: Option<f64>,
}

impl WorldMonitor {
    pub fn new(log: EventLog) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("Vortex/0.1 (persoonlijke wereldmonitor; lokale assistent)")
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { http, log, last_hit: Mutex::new(HashMap::new()) }
    }

    pub fn log(&self) -> &EventLog {
        &self.log
    }

    fn last_ok(&self, name: &str) -> Option<String> {
        self.log
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT last_ok FROM world_sources WHERE name = ?1", [name], |r| {
                    r.get::<_, Option<String>>(0)
                })
                .optional()?
                .flatten())
            })
            .ok()
            .flatten()
    }

    /// Is de bron aan de beurt (interval verstreken)?
    pub fn due(&self, s: &SourceSpec) -> bool {
        let last: Option<String> = self
            .log
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT last_fetch FROM world_sources WHERE name = ?1", [&s.name], |r| {
                    r.get::<_, Option<String>>(0)
                })
                .optional()?
                .flatten())
            })
            .ok()
            .flatten();
        match last.and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok()) {
            Some(t) => (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_minutes() >= s.interval_min as i64,
            None => true,
        }
    }

    /// Tempo per soort bron (minimale tijd tussen verzoeken, zie [`terms`]).
    async fn pace(&self, kind: &str) {
        let (_, min_gap) = terms(kind);
        let wait = {
            let mut g = self.last_hit.lock();
            let now = Instant::now();
            let next = g.get(kind).map(|t| *t + Duration::from_secs(min_gap)).filter(|t| *t > now);
            g.insert(kind.to_string(), next.unwrap_or(now));
            next.map(|t| t - now)
        };
        if let Some(w) = wait {
            tokio::time::sleep(w).await;
        }
    }

    /// Wikidata-koppeling (W14.01/W14.05): namen uit titels van bevestigde events → Q-ID, label,
    /// beschrijving (`world_events.entities`). Geeft het aantal gekoppelde events.
    pub async fn link_entities(&self, base: &str, lang: &str, max_events: usize) -> usize {
        let todo: Vec<(String, String)> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id, title FROM world_events WHERE status = 'confirmed' AND entities = '[]' ORDER BY last_seen DESC LIMIT ?1")?;
                let v = st.query_map([max_events as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let mut n = 0;
        for (id, title) in todo {
            let mut ents = Vec::new();
            for name in names(&title).into_iter().take(4) {
                self.pace("wikidata").await;
                if let Some((qid, label, desc)) = sources::wikidata_lookup(&self.http, base, &name, lang).await {
                    ents.push(serde_json::json!({"name": name, "qid": qid, "label": label, "description": desc}));
                }
            }
            let v = if ents.is_empty() { serde_json::json!([{"name": null}]) } else { serde_json::json!(ents) };
            let _ = self.log.db().write(|tx| {
                tx.execute("UPDATE world_events SET entities = ?2 WHERE id = ?1", params![id, v.to_string()])?;
                Ok(())
            });
            n += usize::from(!ents.is_empty());
        }
        n
    }

    /// Eén bron ophalen (met tempo + inhaal-modus) en opslaan. Nieuwe items krijgen nog geen event.
    pub async fn fetch(&self, s: &SourceSpec) -> FetchReport {
        self.pace(&s.kind).await;
        let since = self
            .last_ok(&s.name)
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
            .map(|t| t.with_timezone(&chrono::Utc));
        let catch_up = since
            .map(|t| (chrono::Utc::now() - t).num_minutes() as f64 / 60.0)
            .filter(|h| *h > s.interval_min as f64 / 60.0 * 1.5);
        let now = vortex_types::now().to_rfc3339();
        let mut rep = FetchReport { source: s.name.clone(), catch_up_hours: catch_up, ..Default::default() };
        match sources::fetch(&self.http, s, since).await {
            Ok(items) => {
                rep.items = items.len();
                let new = self.store(&items);
                rep.new = new;
                let _ = self.log.db().write(|tx| {
                    tx.execute(
                        "INSERT INTO world_sources(name, last_fetch, last_ok, items) VALUES (?1, ?2, ?2, ?3)
                         ON CONFLICT(name) DO UPDATE SET last_fetch = ?2, last_ok = ?2, items = items + ?3, last_error = NULL",
                        params![s.name, now, new as i64],
                    )?;
                    Ok(())
                });
            }
            Err(e) => {
                rep.error = Some(e.clone());
                let _ = self.log.db().write(|tx| {
                    tx.execute(
                        "INSERT INTO world_sources(name, last_fetch, errors, last_error) VALUES (?1, ?2, 1, ?3)
                         ON CONFLICT(name) DO UPDATE SET last_fetch = ?2, errors = errors + 1, last_error = ?3",
                        params![s.name, now, e],
                    )?;
                    Ok(())
                });
            }
        }
        let _ = self.log.append(Event::new(
            EventKind::WorldFetched,
            TraceId::new(),
            serde_json::to_value(&rep).unwrap_or_default(),
        ));
        rep
    }

    /// Items opslaan (dedup op ID). Geeft het aantal nieuwe.
    pub fn store(&self, items: &[Item]) -> usize {
        let now = vortex_types::now().to_rfc3339();
        self.log
            .db()
            .write(|tx| {
                let mut n = 0;
                for it in items {
                    n += tx.execute(
                        "INSERT OR IGNORE INTO world_items(id, source, kind, title, summary, url, published, fetched_at, lang, country, extra) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                        params![it.id, it.source, it.kind, it.title, it.summary, it.url, it.published, now, it.lang, it.country, it.extra.to_string()],
                    )?;
                }
                Ok(n)
            })
            .unwrap_or(0)
    }

    /// Site volgen (recrawl-schema, W14.03).
    pub fn follow(&self, url: &str, interval_hours: f64) -> Result<(), String> {
        self.log
            .db()
            .write(|tx| {
                tx.execute(
                    "INSERT INTO followed_sites(url, interval_hours) VALUES (?1, ?2) ON CONFLICT(url) DO UPDATE SET interval_hours = ?2",
                    params![url, interval_hours.max(0.05)],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn unfollow(&self, url: &str) -> bool {
        self.log
            .db()
            .write(|tx| Ok(tx.execute("DELETE FROM followed_sites WHERE url = ?1", [url])? > 0))
            .unwrap_or(false)
    }

    pub fn followed(&self) -> Vec<serde_json::Value> {
        self.log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT url, interval_hours, last_fetch, changes FROM followed_sites ORDER BY url")?;
                let v = st
                    .query_map([], |r| {
                        Ok(serde_json::json!({"url": r.get::<_, String>(0)?, "interval_hours": r.get::<_, f64>(1)?, "last_fetch": r.get::<_, Option<String>>(2)?, "changes": r.get::<_, i64>(3)?}))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default()
    }

    /// Gevolgde sites die aan de beurt zijn ophalen; bij gewijzigde inhoud → item (kind "page").
    /// JS-pagina's via de headless browser als die er is. Geeft de gewijzigde URL's.
    pub async fn recrawl_due(&self, browser: Option<&std::path::Path>) -> Vec<String> {
        let now = chrono::Utc::now();
        let due: Vec<(String, Option<String>)> = self
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT url, interval_hours, last_fetch, content_hash FROM followed_sites")?;
                let v = st
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, f64>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    })?
                    .filter_map(|x| x.ok())
                    .filter(|(_, iv, last, _)| {
                        last.as_deref()
                            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                            .is_none_or(|t| (now - t.with_timezone(&chrono::Utc)).num_minutes() as f64 >= iv * 60.0)
                    })
                    .map(|(u, _, _, h)| (u, h))
                    .collect();
                Ok(v)
            })
            .unwrap_or_default();
        let mut changed = Vec::new();
        for (url, old) in due {
            let Ok(r) = self.http.get(&url).send().await else { continue };
            let Ok(html) = r.text().await else { continue };
            let base = reqwest::Url::parse(&url).ok();
            let Some(base) = base else { continue };
            let mut ex = crate::extract(&html, &base);
            if let Some(b) = browser
                && crate::crawl::needs_browser(&html, &ex.text)
                && let Some(dom) = crate::crawl::render(b, &url, 30).await
            {
                ex = crate::extract(&dom, &base);
            }
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in ex.text.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            let hash = format!("{h:016x}");
            let is_change = old.as_deref() != Some(hash.as_str());
            let ts = vortex_types::now().to_rfc3339();
            let _ = self.log.db().write(|tx| {
                tx.execute(
                    "UPDATE followed_sites SET last_fetch = ?2, content_hash = ?3, changes = changes + ?4 WHERE url = ?1",
                    params![url, ts, hash, i64::from(is_change && old.is_some())],
                )?;
                Ok(())
            });
            if is_change {
                let title = if ex.title.is_empty() { url.clone() } else { ex.title.clone() };
                let it = Item {
                    id: format!("wi_page_{hash}"),
                    source: format!("site:{}", base.host_str().unwrap_or("")),
                    kind: "page".into(),
                    title,
                    summary: ex.text.chars().take(1500).collect(),
                    url: Some(url.clone()),
                    published: Some(ts.clone()),
                    lang: None,
                    country: None,
                    extra: serde_json::json!({"first": old.is_none()}),
                };
                self.store(std::slice::from_ref(&it));
                changed.push(url);
            }
        }
        changed
    }

    /// Bronbias (W14.05): per bron het aandeel items per regio t.o.v. het gemiddelde, en hoe
    /// vaak een bron bij bevestigde events zit.
    pub fn source_bias(&self) -> Vec<serde_json::Value> {
        self.log
            .db()
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT i.source, COUNT(*),
                        SUM(CASE WHEN e.status = 'confirmed' THEN 1 ELSE 0 END),
                        COUNT(DISTINCT i.country),
                        GROUP_CONCAT(DISTINCT i.country)
                     FROM world_items i LEFT JOIN world_events e ON e.id = i.event_id GROUP BY i.source",
                )?;
                let v = st
                    .query_map([], |r| {
                        let n: i64 = r.get(1)?;
                        let conf: i64 = r.get::<_, Option<i64>>(2)?.unwrap_or(0);
                        Ok(serde_json::json!({
                            "source": r.get::<_, String>(0)?, "items": n,
                            "confirmed_share": if n > 0 { conf as f64 / n as f64 } else { 0.0 },
                            "regions": r.get::<_, i64>(3)?, "countries": r.get::<_, Option<String>>(4)?
                        }))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn candidate_names() {
        assert_eq!(super::names("Strong earthquake strikes coast of Japan"), vec!["Japan"]);
        assert_eq!(
            super::names("European Central Bank raises rates, says Christine Lagarde"),
            vec!["European Central Bank", "Christine Lagarde"]
        );
    }
}
