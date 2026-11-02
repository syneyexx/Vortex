//! Wereldmodel (W14.05–W14.08): bevestigde events → temporal KG (geheugen, kind World, met
//! geldigheid en bron), causale hypotheses met vertrouwen, voorspellingen die later automatisch
//! getoetst worden (kalibratie van het wereldmodel), dag/week/maand-samenvattingen per
//! thema/regio, en spelers per sector (alleen publieke figuren en organisaties).
// VX:W14.05

use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vortex_cognition::GenOpts;
use vortex_core::EventLog;
use vortex_memory::{MemoryKind, MemoryStore, NewMemory};
use vortex_models::backend::Message;
use vortex_neural::calibration::{Calibrator, measure};
use vortex_types::{Event, EventKind, Provenance, SourceKind, TraceId};

use crate::Llm;

#[derive(Debug, Clone)]
pub struct WorldDeps {
    pub log: EventLog,
    pub llm: Llm,
    pub memory: Option<Arc<MemoryStore>>,
}

async fn ask(llm: &Llm, sys: &str, user: String, max: u32) -> Value {
    let out = llm
        .complete(
            vec![Message::system(sys.to_string()), Message::user(user)],
            GenOpts { max_tokens: Some(max), temperature: Some(0.2), json_schema: None, reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    crate::json::extract(&out).unwrap_or_default()
}

fn hid(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn emit(log: &EventLog, kind: EventKind, payload: Value) {
    let _ = log.append(Event::new(kind, TraceId::new(), payload));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub first_seen: String,
    pub last_seen: String,
    pub status: String,
    pub n_items: i64,
    pub region: Option<String>,
    pub theme: Option<String>,
    pub memory_id: Option<String>,
    /// Wikidata-koppeling: [{name, qid, label, description}].
    #[serde(default)]
    pub entities: Value,
}

pub fn events(log: &EventLog, since: &str, status: Option<&str>, limit: usize) -> Vec<WorldEvent> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(
                "SELECT id, title, summary, first_seen, last_seen, status, n_items, region, theme, memory_id, entities FROM world_events
                 WHERE last_seen >= ?1 AND (?2 IS NULL OR status = ?2) ORDER BY n_items DESC, last_seen DESC LIMIT ?3",
            )?;
            let v = st
                .query_map(params![since, status, limit as i64], |r| {
                    Ok(WorldEvent {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        summary: r.get(2)?,
                        first_seen: r.get(3)?,
                        last_seen: r.get(4)?,
                        status: r.get(5)?,
                        n_items: r.get(6)?,
                        region: r.get(7)?,
                        theme: r.get(8)?,
                        memory_id: r.get(9)?,
                        entities: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or_default(),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

fn event_sources(log: &EventLog, id: &str) -> Vec<(String, Option<String>)> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT source, url FROM world_items WHERE event_id = ?1 ORDER BY published")?;
            let v = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

/// W14.05: bevestigde events zonder herinnering → geheugen (kind World). De schrijfpijplijn
/// haalt entiteiten en claims eruit (temporal KG), met `valid_from` = eerste melding.
pub async fn integrate(d: &WorldDeps, max: usize) -> usize {
    let Some(mem) = &d.memory else { return 0 };
    let since = (vortex_types::now() - chrono::Duration::days(30)).to_rfc3339();
    let mut n = 0;
    for e in events(&d.log, &since, Some("confirmed"), 500).into_iter().filter(|e| e.memory_id.is_none()).take(max) {
        let srcs = event_sources(&d.log, &e.id);
        let names: Vec<String> = {
            let mut v: Vec<String> = srcs.iter().map(|s| s.0.clone()).collect();
            v.dedup();
            v
        };
        let ents: Vec<String> = e
            .entities
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| {
                Some(format!(
                    "{} ({}, {})",
                    x["label"].as_str()?,
                    x["qid"].as_str()?,
                    x["description"].as_str().unwrap_or("")
                ))
            })
            .collect();
        let content = format!(
            "{} ({}). {} Bronnen: {}.{}",
            e.title,
            e.first_seen.get(..10).unwrap_or(""),
            e.summary,
            names.join(", "),
            if ents.is_empty() { String::new() } else { format!(" Entiteiten: {}.", ents.join("; ")) }
        );
        let mut m = NewMemory::new(
            MemoryKind::World,
            content,
            Provenance::with_origin(SourceKind::Web, format!("world:{}", e.id)),
        );
        m.title = e.title.clone();
        m.source_uri = srcs.iter().find_map(|s| s.1.clone());
        m.source_hash = Some(format!("world:{}", e.id));
        m.event_time = Some(e.first_seen.clone());
        m.valid_from = Some(e.first_seen.clone());
        m.confidence = Some((0.6 + 0.1 * (names.len() as f64)).min(0.95));
        m.tags = ["wereld".to_string(), format!("thema:{}", e.theme.clone().unwrap_or_default())]
            .into_iter()
            .chain(e.region.clone().map(|r| format!("regio:{r}")))
            .collect();
        m.force = true;
        if let Ok(Some(o)) = mem.write_or_defer(m).await {
            let mid = match o {
                vortex_memory::WriteOutcome::Stored { ids, .. } => ids.first().map(|x| x.to_string()),
                vortex_memory::WriteOutcome::Merged { into } => Some(into.to_string()),
                vortex_memory::WriteOutcome::Skipped { .. } => None,
            };
            let _ = d.log.db().write(|tx| {
                tx.execute("UPDATE world_events SET memory_id = ?2 WHERE id = ?1", params![e.id, mid])?;
                Ok(())
            });
            n += 1;
        }
    }
    n
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub cause: String,
    pub effect: String,
    pub confidence: f64,
    pub support: i64,
    pub contra: i64,
    pub last_seen: String,
}

// VX:W14.06
/// W14.06: causale hypotheses uit recente bevestigde events. Bestaande hypothese opnieuw
/// gezien → steun +1 en vertrouwen omhoog; tegengesproken → contra +1.
pub async fn hypothesize(d: &WorldDeps, window_h: i64) -> Vec<Hypothesis> {
    let since = (vortex_types::now() - chrono::Duration::hours(window_h)).to_rfc3339();
    let evs = events(&d.log, &since, Some("confirmed"), 25);
    if evs.is_empty() {
        return vec![];
    }
    let list = evs
        .iter()
        .enumerate()
        .map(|(i, e)| format!("[{i}] {} — {}", e.title, e.summary.chars().take(200).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");
    let known =
        hypotheses(&d.log, 30).iter().map(|h| format!("{} → {}", h.cause, h.effect)).collect::<Vec<_>>().join("\n");
    let v = ask(
        &d.llm,
        "Je bent de wereldmodel-analist. Zoek plausibele oorzaak→gevolg-verbanden tussen of uit deze events (geen speculatie zonder grond). \
         Geef ook aan of een bekende hypothese bevestigd of tegengesproken wordt. JSON: {\"hypotheses\": [{\"cause\": \"…\", \"effect\": \"…\", \"confidence\": 0..1, \"events\": [i]}], \
         \"known\": [{\"cause\": \"…\", \"effect\": \"…\", \"verdict\": \"support|contra\"}]}",
        format!("Events:\n{list}\n\nBekende hypotheses:\n{known}"),
        700,
    )
    .await;
    let now = vortex_types::now().to_rfc3339();
    let _ = d.log.db().write(|tx| {
        for h in v["hypotheses"].as_array().into_iter().flatten() {
            let (Some(c), Some(e)) = (h["cause"].as_str(), h["effect"].as_str()) else { continue };
            let conf = h["confidence"].as_f64().unwrap_or(0.5).clamp(0.05, 0.95);
            tx.execute(
                "INSERT INTO hypotheses(id, cause, effect, confidence, support, contra, last_seen) VALUES (?1, ?2, ?3, ?4, 1, 0, ?5)
                 ON CONFLICT(cause, effect) DO UPDATE SET support = support + 1, confidence = MIN(0.97, (confidence * support + ?4) / (support + 1) + 0.03), last_seen = ?5",
                params![format!("hy_{}", hid(&format!("{c}|{e}"))), c, e, conf, now],
            )?;
        }
        for k in v["known"].as_array().into_iter().flatten() {
            let (Some(c), Some(e)) = (k["cause"].as_str(), k["effect"].as_str()) else { continue };
            let sql = if k["verdict"] == "contra" {
                "UPDATE hypotheses SET contra = contra + 1, confidence = MAX(0.03, confidence - 0.1), last_seen = ?3 WHERE cause = ?1 AND effect = ?2"
            } else {
                "UPDATE hypotheses SET support = support + 1, confidence = MIN(0.97, confidence + 0.05), last_seen = ?3 WHERE cause = ?1 AND effect = ?2"
            };
            tx.execute(sql, params![c, e, now])?;
        }
        Ok(())
    });
    hypotheses(&d.log, 50)
}

pub fn hypotheses(log: &EventLog, limit: usize) -> Vec<Hypothesis> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, cause, effect, confidence, support, contra, last_seen FROM hypotheses ORDER BY confidence * support DESC LIMIT ?1")?;
            let v = st
                .query_map([limit as i64], |r| {
                    Ok(Hypothesis { id: r.get(0)?, cause: r.get(1)?, effect: r.get(2)?, confidence: r.get(3)?, support: r.get(4)?, contra: r.get(5)?, last_seen: r.get(6)? })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub id: String,
    pub claim: String,
    pub probability: f64,
    pub criteria: String,
    pub resolve_by: String,
    pub actor: Option<String>,
    pub origin: String,
    pub status: String,
    pub evidence: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

/// Voorspelling vastleggen (toetsbaar: criteria + datum).
pub fn add_prediction(
    log: &EventLog,
    claim: &str,
    probability: f64,
    criteria: &str,
    resolve_by: &str,
    actor: Option<&str>,
    origin: &str,
) -> Result<Prediction, String> {
    if claim.trim().is_empty() || criteria.trim().is_empty() {
        return Err("claim en toetscriteria zijn verplicht".into());
    }
    let resolve_by = chrono::DateTime::parse_from_rfc3339(resolve_by)
        .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339())
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(resolve_by, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(23, 59, 0).unwrap_or_default().and_utc().to_rfc3339())
        })
        .map_err(|_| format!("ongeldige datum {resolve_by}"))?;
    let now = vortex_types::now().to_rfc3339();
    let p = Prediction {
        id: format!("pr_{}", hid(&format!("{claim}|{resolve_by}|{}", actor.unwrap_or("")))),
        claim: claim.trim().into(),
        probability: probability.clamp(0.01, 0.99),
        criteria: criteria.trim().into(),
        resolve_by,
        actor: actor.map(String::from),
        origin: origin.into(),
        status: "open".into(),
        evidence: None,
        created_at: now,
        resolved_at: None,
    };
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO predictions(id, claim, probability, criteria, resolve_by, actor, origin, status, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'open',?8)",
                params![p.id, p.claim, p.probability, p.criteria, p.resolve_by, p.actor, p.origin, p.created_at],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    emit(log, EventKind::PredictionMade, serde_json::to_value(&p).unwrap_or_default());
    Ok(p)
}

/// W14.06: voorspellingen genereren uit events + sterke hypotheses.
pub async fn predict(d: &WorldDeps, horizon_days: i64, n: usize) -> Vec<Prediction> {
    let since = (vortex_types::now() - chrono::Duration::days(3)).to_rfc3339();
    let evs = events(&d.log, &since, Some("confirmed"), 20);
    let hyps = hypotheses(&d.log, 10);
    if evs.is_empty() && hyps.is_empty() {
        return vec![];
    }
    let today = chrono::Utc::now().date_naive();
    let ctx = format!(
        "Vandaag: {today}. Horizon: {horizon_days} dagen.\nEvents:\n{}\nHypotheses:\n{}",
        evs.iter()
            .map(|e| format!("- {} ({})", e.title, e.theme.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n"),
        hyps.iter()
            .map(|h| format!("- {} → {} ({:.0}%)", h.cause, h.effect, h.confidence * 100.0))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let v = ask(
        &d.llm,
        &format!("Doe maximaal {n} concrete, later objectief toetsbare voorspellingen (geen vaagheden). Elke voorspelling: claim, kans 0..1 (gekalibreerd: niet alles 0.9), toetscriteria (wat telt als waar) en datum (YYYY-MM-DD, binnen de horizon). JSON: {{\"predictions\": [{{\"claim\": \"…\", \"probability\": 0.6, \"criteria\": \"…\", \"resolve_by\": \"YYYY-MM-DD\"}}]}}"),
        ctx,
        700,
    )
    .await;
    v["predictions"]
        .as_array()
        .into_iter()
        .flatten()
        .take(n)
        .filter_map(|p| {
            add_prediction(
                &d.log,
                p["claim"].as_str()?,
                p["probability"].as_f64().unwrap_or(0.5),
                p["criteria"].as_str()?,
                p["resolve_by"].as_str()?,
                None,
                "world-model",
            )
            .ok()
        })
        .collect()
}

pub fn predictions(log: &EventLog, status: Option<&str>, limit: usize) -> Vec<Prediction> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, claim, probability, criteria, resolve_by, actor, origin, status, evidence, created_at, resolved_at FROM predictions WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC LIMIT ?2")?;
            let v = st
                .query_map(params![status, limit as i64], |r| {
                    Ok(Prediction {
                        id: r.get(0)?,
                        claim: r.get(1)?,
                        probability: r.get(2)?,
                        criteria: r.get(3)?,
                        resolve_by: r.get(4)?,
                        actor: r.get(5)?,
                        origin: r.get(6)?,
                        status: r.get(7)?,
                        evidence: r.get(8)?,
                        created_at: r.get(9)?,
                        resolved_at: r.get(10)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

/// (titel, samenvatting, url)
type ItemRow = (String, String, Option<String>);

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(String::from)
        .collect()
}

/// W14.06: verlopen voorspellingen toetsen tegen wat er sindsdien binnenkwam (items + events).
pub async fn resolve_due(d: &WorldDeps) -> Vec<Prediction> {
    let now = vortex_types::now().to_rfc3339();
    let due: Vec<Prediction> =
        predictions(&d.log, Some("open"), 1000).into_iter().filter(|p| p.resolve_by <= now).collect();
    let mut out = Vec::new();
    for p in due {
        let kw = words(&format!("{} {}", p.claim, p.criteria));
        let items: Vec<ItemRow> = d
            .log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT title, summary, url FROM world_items WHERE COALESCE(published, fetched_at) >= ?1 ORDER BY COALESCE(published, fetched_at) DESC LIMIT 3000")?;
                let v = st.query_map([&p.created_at], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let mut scored: Vec<(usize, &ItemRow)> = items
            .iter()
            .map(|it| {
                let t = format!("{} {}", it.0, it.1).to_lowercase();
                (kw.iter().filter(|w| t.contains(w.as_str())).count(), it)
            })
            .filter(|x| x.0 >= 2)
            .collect();
        scored.sort_by_key(|x| std::cmp::Reverse(x.0));
        let evidence: Vec<String> = scored
            .iter()
            .take(8)
            .map(|(_, it)| {
                format!(
                    "- {} {} {}",
                    it.0,
                    it.1.chars().take(200).collect::<String>(),
                    it.2.clone().unwrap_or_default()
                )
            })
            .collect();
        let (status, why) = if evidence.is_empty() {
            ("unresolvable".to_string(), "geen relevant bewijs gevonden in de wereldmonitor".to_string())
        } else {
            let v = ask(
                &d.llm,
                "Toets deze voorspelling strikt aan de criteria en het bewijs. Alleen 'true' of 'false' als het bewijs het echt beslist, anders 'unresolvable'. JSON: {\"verdict\": \"true|false|unresolvable\", \"why\": \"…\"}",
                format!("Voorspelling: {}\nCriteria: {}\nDeadline: {}\nBewijs:\n{}", p.claim, p.criteria, p.resolve_by, evidence.join("\n")),
                200,
            )
            .await;
            let s = v["verdict"]
                .as_str()
                .filter(|s| ["true", "false", "unresolvable"].contains(s))
                .unwrap_or("unresolvable")
                .to_string();
            (s, v["why"].as_str().unwrap_or("").to_string())
        };
        let ev = json!({"why": why, "evidence": evidence});
        let _ = d.log.db().write(|tx| {
            tx.execute(
                "UPDATE predictions SET status = ?2, evidence = ?3, resolved_at = ?4 WHERE id = ?1",
                params![p.id, status, ev.to_string(), now],
            )?;
            Ok(())
        });
        let mut r = p.clone();
        r.status = status;
        r.evidence = Some(ev.to_string());
        r.resolved_at = Some(now.clone());
        emit(
            &d.log,
            EventKind::PredictionResolved,
            json!({"id": r.id, "claim": r.claim, "probability": r.probability, "status": r.status, "actor": r.actor}),
        );
        out.push(r);
    }
    out
}

/// Kalibratie van het wereldmodel (alleen beslist waar/onwaar), totaal en per herkomst/speler.
pub fn calibration(log: &EventLog) -> Value {
    let rows: Vec<(f64, String, String)> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT probability, status, COALESCE(actor, origin) FROM predictions WHERE status IN ('true','false') ORDER BY resolved_at")?;
            let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    let data: Vec<(f64, f64)> = rows.iter().map(|(p, s, _)| (*p, if s == "true" { 1.0 } else { 0.0 })).collect();
    let mut by: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for ((_, _, k), x) in rows.iter().zip(&data) {
        by.entry(k.clone()).or_default().push(*x);
    }
    let counts: Value = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT status, COUNT(*) FROM predictions GROUP BY status")?;
            let m = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
            Ok(json!(m))
        })
        .unwrap_or_default();
    json!({
        "overall": measure(&data, |p| p),
        "by": by.iter().map(|(k, v)| (k.clone(), json!(measure(v, |p| p)))).collect::<serde_json::Map<_, _>>(),
        "calibrator": (data.len() >= 20).then(|| Calibrator::fit(&data)),
        "counts": counts,
    })
}

// VX:W14.07
/// W14.07: samenvatting per thema of regio voor dag/week/maand (ook als herinnering, kind Summary).
pub async fn summarize(d: &WorldDeps, period: &str, group_by: &str) -> Vec<Value> {
    let days = match period {
        "week" => 7,
        "month" => 30,
        _ => 1,
    };
    let since = (vortex_types::now() - chrono::Duration::days(days)).to_rfc3339();
    let mut groups: BTreeMap<String, Vec<WorldEvent>> = BTreeMap::new();
    for e in events(&d.log, &since, None, 400) {
        let k =
            if group_by == "region" { e.region.clone() } else { e.theme.clone() }.unwrap_or_else(|| "overig".into());
        groups.entry(k).or_default().push(e);
    }
    let mut out = Vec::new();
    for (k, evs) in groups {
        let confirmed = evs.iter().filter(|e| e.status == "confirmed").count();
        let list = evs
            .iter()
            .take(25)
            .map(|e| {
                format!(
                    "- [{}] {} ({} bronnen)",
                    if e.status == "confirmed" { "bevestigd" } else { "gemeld" },
                    e.title,
                    e.n_items
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let v = ask(
            &d.llm,
            "Vat de belangrijkste ontwikkelingen samen in 3–6 zinnen, Nederlands. Bevestigd en alleen-gemeld duidelijk scheiden; filter op belang. JSON: {\"summary\": \"…\", \"top\": [\"…\"]}",
            format!("Periode: {period}, {group_by}: {k}\n{list}"),
            500,
        )
        .await;
        let summary = v["summary"].as_str().map(String::from).unwrap_or_else(|| {
            // Zonder model: extractief (grootste events eerst).
            evs.iter().take(5).map(|e| e.title.clone()).collect::<Vec<_>>().join(" · ")
        });
        if let Some(mem) = &d.memory {
            let mut m = NewMemory::new(
                MemoryKind::Summary,
                format!("Wereld {period} — {k}: {summary}"),
                Provenance::with_origin(SourceKind::System, "world-summary"),
            );
            m.title = format!("Wereld {period} · {k} · {}", chrono::Utc::now().date_naive());
            m.source_hash = Some(format!("world-summary:{period}:{group_by}:{k}:{}", chrono::Utc::now().date_naive()));
            m.tags = vec!["wereld".into(), format!("periode:{period}"), format!("{group_by}:{k}")];
            m.valid_from = Some(since.clone());
            m.force = true;
            let _ = mem.write_or_defer(m).await;
        }
        out.push(json!({"key": k, "period": period, "group_by": group_by, "events": evs.len(), "confirmed": confirmed, "summary": summary, "top": v["top"]}));
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub sector: String,
    /// person (alleen publieke figuren, met publieke rol) | organization
    pub kind: String,
    pub sources: Vec<String>,
    pub notes: String,
}

// VX:W14.08
/// W14.08: speler toevoegen. Alleen organisaties en publieke figuren in hun publieke rol
/// (bv. "CEO van X", "minister van Y"); geen privépersonen.
pub fn add_player(log: &EventLog, name: &str, sector: &str, kind: &str, public_role: &str) -> Result<Player, String> {
    if !["person", "organization"].contains(&kind) {
        return Err("kind = person | organization".into());
    }
    if kind == "person" && public_role.trim().len() < 3 {
        return Err("personen alleen als publieke figuur: geef hun publieke rol (bv. \"CEO van …\")".into());
    }
    let p = Player {
        id: format!("pl_{}", hid(name.trim())),
        name: name.trim().into(),
        sector: sector.trim().into(),
        kind: kind.into(),
        sources: vec![],
        notes: public_role.trim().into(),
    };
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO players(id, name, sector, kind, notes) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET sector = ?3, notes = ?5",
                params![p.id, p.name, p.sector, p.kind, p.notes],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(p)
}

pub fn players(log: &EventLog) -> Vec<Player> {
    log.db()
        .read(|c| {
            let mut st =
                c.prepare("SELECT id, name, sector, kind, sources, notes FROM players ORDER BY sector, name")?;
            let v = st
                .query_map([], |r| {
                    let s: String = r.get(4)?;
                    Ok(Player {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        sector: r.get(2)?,
                        kind: r.get(3)?,
                        sources: serde_json::from_str(&s).unwrap_or_default(),
                        notes: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

pub fn player(log: &EventLog, id: &str) -> Option<Player> {
    players(log).into_iter().find(|p| p.id == id)
}

/// W14.08: bronnen verzamelen (wereldmonitor + ingelezen documenten in het geheugen, bv.
/// interviews, aandeelhoudersbrieven, jaarverslagen) en beslissingen/context/uitkomst/principes
/// + mislukkingen extraheren → geheugen (World, met bron en locator). Geeft de records.
pub async fn study_player(d: &WorldDeps, id: &str) -> Result<Vec<Value>, String> {
    let p = player(&d.log, id).ok_or("onbekende speler")?;
    let like = format!("%{}%", p.name.to_lowercase());
    let mut passages: Vec<(String, String)> = d
        .log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT COALESCE(url, source), title || '. ' || summary FROM world_items WHERE lower(title || ' ' || summary) LIKE ?1 ORDER BY published DESC LIMIT 30")?;
            let v = st.query_map([&like], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    if let Some(mem) = &d.memory {
        let f = vortex_memory::Filters { include_stale: false, ..Default::default() };
        for m in mem.list(&f, Some(&p.name), 30, 0).unwrap_or_default() {
            if matches!(m.kind, MemoryKind::User | MemoryKind::Project) {
                continue;
            }
            passages.push((
                m.source_uri.clone().unwrap_or_else(|| format!("memory:{}", m.id)),
                m.content.chars().take(1200).collect(),
            ));
        }
    }
    if passages.is_empty() {
        return Err(format!(
            "nog geen bronnen over {} (lees interviews/jaarverslagen in of laat de wereldmonitor lopen)",
            p.name
        ));
    }
    let list = passages
        .iter()
        .enumerate()
        .map(|(i, (_, t))| format!("[{i}] {}", t.chars().take(700).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");
    let v = ask(
        &d.llm,
        "Extraheer uit de bronnen, alleen over de publieke rol/het handelen van deze speler: beslissingen (met context en uitkomst), principes, en ook mislukkingen (tegen survivorship bias). \
         Alleen wat in de bron staat. JSON: {\"records\": [{\"type\": \"decision|principle|failure|outcome\", \"text\": \"…\", \"context\": \"…\", \"outcome\": \"…\", \"date\": \"YYYY-MM-DD|null\", \"source\": i}]}",
        format!("Speler: {} ({}, {})\nBronnen:\n{list}", p.name, p.sector, p.notes),
        1200,
    )
    .await;
    let recs: Vec<Value> = v["records"].as_array().cloned().unwrap_or_default();
    let mut sources: Vec<String> = p.sources.clone();
    for r in &recs {
        let Some(text) = r["text"].as_str() else { continue };
        let src = r["source"].as_u64().and_then(|i| passages.get(i as usize)).map(|x| x.0.clone());
        if let Some(s) = &src
            && !sources.contains(s)
        {
            sources.push(s.clone());
        }
        if let Some(mem) = &d.memory {
            let ty = r["type"].as_str().unwrap_or("decision");
            let content = format!(
                "{} — {ty}: {text}{}{}",
                p.name,
                r["context"].as_str().map(|c| format!(" Context: {c}.")).unwrap_or_default(),
                r["outcome"].as_str().map(|c| format!(" Uitkomst: {c}.")).unwrap_or_default()
            );
            let mut m = NewMemory::new(
                MemoryKind::World,
                content,
                Provenance::with_origin(SourceKind::Web, src.clone().unwrap_or_default()),
            );
            m.title = format!("{} · {ty}", p.name);
            m.source_uri = src;
            m.locator = r["source"].as_u64().map(|i| format!("passage {i}"));
            m.event_time = r["date"].as_str().filter(|d| d.len() == 10).map(|d| format!("{d}T00:00:00+00:00"));
            m.tags = vec![format!("speler:{}", p.id), format!("sector:{}", p.sector), format!("type:{ty}")];
            let _ = mem.write_or_defer(m).await;
        }
    }
    let _ = d.log.db().write(|tx| {
        tx.execute(
            "UPDATE players SET sources = ?2 WHERE id = ?1",
            params![p.id, serde_json::to_string(&sources).unwrap_or_default()],
        )?;
        Ok(())
    });
    Ok(recs)
}

/// "Wat zou X doen?": voorspelling op basis van de principes/beslissingen van de speler, die
/// later getoetst wordt (zelfde mechanisme als het wereldmodel; `actor` = speler).
pub async fn predict_player(d: &WorldDeps, id: &str, situation: &str, horizon_days: i64) -> Result<Prediction, String> {
    let p = player(&d.log, id).ok_or("onbekende speler")?;
    let mut known = String::new();
    if let Some(mem) = &d.memory {
        let f = vortex_memory::Filters::default();
        for m in mem.list(&f, Some(&p.name), 15, 0).unwrap_or_default() {
            if m.tags.iter().any(|t| t == &format!("speler:{}", p.id)) {
                known.push_str(&format!("- {}\n", m.content.chars().take(300).collect::<String>()));
            }
        }
    }
    let by = (chrono::Utc::now() + chrono::Duration::days(horizon_days)).date_naive();
    let v = ask(
        &d.llm,
        "Voorspel wat deze speler in deze situatie doet, op basis van hun eerdere beslissingen en principes. Toetsbaar formuleren. JSON: {\"claim\": \"…\", \"probability\": 0..1, \"criteria\": \"…\"}",
        format!("Speler: {} ({})\nBekend:\n{known}\nSituatie: {situation}\nDeadline: {by}", p.name, p.sector),
        300,
    )
    .await;
    add_prediction(
        &d.log,
        v["claim"].as_str().ok_or("model gaf geen voorspelling")?,
        v["probability"].as_f64().unwrap_or(0.5),
        v["criteria"].as_str().unwrap_or(situation),
        &by.to_string(),
        Some(&p.id),
        "player",
    )
}

/// Eén gebeurtenis (of item) ophalen voor de GUI.
pub fn event_detail(log: &EventLog, id: &str) -> Option<Value> {
    let e = log
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT id, title, summary, first_seen, last_seen, status, n_items, groups, region, theme, memory_id, entities FROM world_events WHERE id = ?1", [id], |r| {
                Ok(json!({"entities": serde_json::from_str::<Value>(&r.get::<_, String>(11)?).unwrap_or_default(), "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "summary": r.get::<_, String>(2)?, "first_seen": r.get::<_, String>(3)?, "last_seen": r.get::<_, String>(4)?, "status": r.get::<_, String>(5)?, "n_items": r.get::<_, i64>(6)?, "groups": serde_json::from_str::<Value>(&r.get::<_, String>(7)?).unwrap_or_default(), "region": r.get::<_, Option<String>>(8)?, "theme": r.get::<_, Option<String>>(9)?, "memory_id": r.get::<_, Option<String>>(10)?}))
            })
            .optional()?)
        })
        .ok()
        .flatten()?;
    let items: Vec<Value> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT source, title, url, published, country FROM world_items WHERE event_id = ?1 ORDER BY published")?;
            let v = st
                .query_map([id], |r| Ok(json!({"source": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "url": r.get::<_, Option<String>>(2)?, "published": r.get::<_, Option<String>>(3)?, "country": r.get::<_, Option<String>>(4)?})))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    let mut e = e;
    e["items"] = json!(items);
    Some(e)
}
