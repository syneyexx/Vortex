//! Event-clustering (W14.04): artikelen over hetzelfde gebeuren → één event. Tokens van titel +
//! samenvatting (zonder stopwoorden), Jaccard-gelijkenis binnen een tijdvenster; een event is
//! **bevestigd** zodra ≥ 2 onafhankelijke brongroepen (verschillend domein/bron) het melden,
//! anders **gemeld**. Thema en regio volgen uit trefwoorden en het land van de bronnen.
// VX:W14.04

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, TraceId};

const STOP: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "have", "has", "was", "were", "are", "will", "after", "over",
    "into", "about", "says", "said", "their", "they", "been", "more", "than", "what", "when", "which", "while", "amid",
    "news", "voor", "naar", "door", "over", "zijn", "werd", "wordt", "heeft", "hebben", "niet", "maar", "deze",
    "onder", "tegen", "bij", "met", "een", "het", "van", "dat", "die", "ook", "nog", "meer",
];

pub fn tokens(s: &str) -> BTreeSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(w))
        .map(|w| {
            // Ruwe stam: meervoud/verleden tijd weg, zodat "quakes"/"quake" samenvallen.
            let w = w.trim_end_matches("ing").trim_end_matches("ed");
            w.strip_suffix('s').filter(|x| x.len() >= 3).unwrap_or(w).to_string()
        })
        .collect()
}

pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let i = a.intersection(b).count() as f64;
    i / (a.len() + b.len()) as f64 * 2.0 // Dice: robuuster bij korte koppen
}

pub fn theme(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| t.contains(w));
    if has(&[
        "earthquake",
        "aardbeving",
        "quake",
        "flood",
        "overstroming",
        "wildfire",
        "bosbrand",
        "storm",
        "volcano",
        "vulkaan",
        "hurricane",
        "cyclone",
        "drought",
    ]) {
        "ramp & natuur"
    } else if has(&[
        "war",
        "oorlog",
        "attack",
        "aanval",
        "missile",
        "troops",
        "ceasefire",
        "conflict",
        "militair",
        "military",
    ]) {
        "conflict & veiligheid"
    } else if has(&[
        "election",
        "verkiezing",
        "parliament",
        "parlement",
        "minister",
        "president",
        "government",
        "regering",
        "kabinet",
        "vote",
    ]) {
        "politiek"
    } else if has(&[
        "inflation",
        "inflatie",
        "gdp",
        "bbp",
        "market",
        "beurs",
        "economy",
        "economie",
        "rates",
        "rente",
        "unemployment",
        "werkloosheid",
        "trade",
    ]) {
        "economie"
    } else if has(&[
        "model",
        "llm",
        " ai ",
        "neural",
        "arxiv",
        "software",
        "chip",
        "openai",
        "anthropic",
        "robot",
        "quantum",
    ]) {
        "technologie & AI"
    } else if has(&["vaccine", "virus", "outbreak", "uitbraak", "health", "gezondheid", "disease", "ziekte", "who "]) {
        "gezondheid"
    } else if has(&["climate", "klimaat", "emission", "uitstoot", "co2"]) {
        "klimaat"
    } else {
        "overig"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterReport {
    pub assigned: usize,
    pub new_events: usize,
    pub confirmed: usize,
}

#[derive(Debug, Clone)]
struct Row {
    id: String,
    source: String,
    title: String,
    summary: String,
    published: String,
    country: Option<String>,
    group: String,
}

/// Niet-geclusterde items van de laatste `window_h` uur aan events koppelen of nieuwe maken.
pub fn cluster(log: &EventLog, window_h: i64, threshold: f64) -> ClusterReport {
    let since = (vortex_types::now() - chrono::Duration::hours(window_h)).to_rfc3339();
    let rows: Vec<Row> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, source, title, summary, COALESCE(published, fetched_at), country, extra FROM world_items WHERE event_id IS NULL AND COALESCE(published, fetched_at) >= ?1 AND kind IN ('article','event','story') ORDER BY COALESCE(published, fetched_at)")?;
            let v = st
                .query_map([&since], |r| {
                    let src: String = r.get(1)?;
                    let extra: String = r.get(6)?;
                    let dom = serde_json::from_str::<serde_json::Value>(&extra).ok().and_then(|e| e["domain"].as_str().map(String::from));
                    Ok(Row { id: r.get(0)?, group: dom.unwrap_or_else(|| src.clone()), source: src, title: r.get(2)?, summary: r.get(3)?, published: r.get(4)?, country: r.get(5)? })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    // Bestaande recente events + hun tokens (titels van de leden).
    struct Ev {
        id: String,
        toks: BTreeSet<String>,
        groups: BTreeSet<String>,
        countries: Vec<String>,
        title: String,
        first: String,
        last: String,
        n: usize,
        new: bool,
        changed: bool,
    }
    let mut evs: Vec<Ev> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT e.id, e.title, e.groups, e.first_seen, e.last_seen, e.n_items, GROUP_CONCAT(i.title, ' '), GROUP_CONCAT(i.country) FROM world_events e LEFT JOIN world_items i ON i.event_id = e.id WHERE e.last_seen >= ?1 GROUP BY e.id")?;
            let v = st
                .query_map([&since], |r| {
                    let groups: String = r.get(2)?;
                    Ok(Ev {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        groups: serde_json::from_str(&groups).unwrap_or_default(),
                        first: r.get(3)?,
                        last: r.get(4)?,
                        n: r.get::<_, i64>(5)? as usize,
                        toks: tokens(&r.get::<_, Option<String>>(6)?.unwrap_or_default()),
                        countries: r.get::<_, Option<String>>(7)?.unwrap_or_default().split(',').filter(|x| !x.is_empty()).map(String::from).collect(),
                        new: false,
                        changed: false,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    let mut assign: Vec<(String, String)> = Vec::new();
    for r in &rows {
        let t = tokens(&format!("{} {}", r.title, r.summary.chars().take(300).collect::<String>()));
        let tt = tokens(&r.title);
        let best = evs
            .iter()
            .enumerate()
            .map(|(i, e)| (i, jaccard(&tt, &e.toks).max(jaccard(&t, &e.toks) * 0.9)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let idx = match best {
            Some((i, s)) if s >= threshold => i,
            _ => {
                evs.push(Ev {
                    id: format!("we_{}", &r.id[3..]),
                    toks: BTreeSet::new(),
                    groups: BTreeSet::new(),
                    countries: vec![],
                    title: r.title.clone(),
                    first: r.published.clone(),
                    last: r.published.clone(),
                    n: 0,
                    new: true,
                    changed: true,
                });
                evs.len() - 1
            }
        };
        let e = &mut evs[idx];
        e.toks.extend(tt);
        e.groups.insert(r.group.clone());
        if let Some(c) = &r.country {
            e.countries.push(c.clone());
        }
        e.n += 1;
        e.last = e.last.clone().max(r.published.clone());
        e.first = e.first.clone().min(r.published.clone());
        e.changed = true;
        assign.push((r.id.clone(), e.id.clone()));
        let _ = &r.source;
    }
    let mut rep =
        ClusterReport { assigned: assign.len(), new_events: evs.iter().filter(|e| e.new).count(), confirmed: 0 };
    let titles: HashMap<String, String> = rows.iter().map(|r| (r.id.clone(), r.title.clone())).collect();
    let _ = log.db().write(|tx| {
        for e in evs.iter().filter(|e| e.changed) {
            let status = if e.groups.len() >= 2 { "confirmed" } else { "reported" };
            let region = mode(&e.countries);
            let members: Vec<&String> = assign.iter().filter(|a| a.1 == e.id).filter_map(|a| titles.get(&a.0)).collect();
            let th = theme(&format!(" {} {} ", e.title, members.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")));
            let summary = members.iter().take(3).map(|s| s.as_str()).collect::<Vec<_>>().join(" · ");
            tx.execute(
                "INSERT INTO world_events(id, title, summary, first_seen, last_seen, status, n_items, groups, region, theme) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT(id) DO UPDATE SET last_seen = ?5, first_seen = ?4, status = ?6, n_items = ?7, groups = ?8, region = COALESCE(?9, region), theme = ?10",
                params![e.id, e.title, summary, e.first, e.last, status, e.n as i64, serde_json::to_string(&e.groups).unwrap_or_default(), region, th],
            )?;
        }
        for (item, ev) in &assign {
            tx.execute("UPDATE world_items SET event_id = ?2 WHERE id = ?1", params![item, ev])?;
        }
        Ok(())
    });
    for e in evs.iter().filter(|e| e.changed) {
        let confirmed = e.groups.len() >= 2;
        if confirmed {
            rep.confirmed += 1;
        }
        let _ = log.append(Event::new(EventKind::WorldEvent, TraceId::new(), serde_json::json!({"id": e.id, "title": e.title, "status": if confirmed { "confirmed" } else { "reported" }, "n_items": e.n, "groups": e.groups})));
    }
    rep
}

fn mode(v: &[String]) -> Option<String> {
    let mut m: HashMap<&String, usize> = HashMap::new();
    for x in v {
        *m.entry(x).or_default() += 1;
    }
    let distinct: HashSet<&String> = v.iter().collect();
    if distinct.len() > 3 {
        return Some("wereldwijd".into());
    }
    m.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|x| x.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_and_theme() {
        let a = tokens("Strong earthquake strikes off coast of Japan");
        let b = tokens("Earthquake of magnitude 7 strikes Japan coast");
        let c = tokens("Dutch cabinet presents budget for 2027");
        assert!(jaccard(&a, &b) > 0.5, "{}", jaccard(&a, &b));
        assert!(jaccard(&a, &c) < 0.1);
        assert_eq!(theme(" strong earthquake japan "), "ramp & natuur");
        assert_eq!(theme(" kabinet presenteert begroting "), "politiek");
    }
}
