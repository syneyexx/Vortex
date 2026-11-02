//! Dagelijkse briefing (W15.06): wereld, wat Vortex leerde, open conflicten, voortgang van taken
//! en projecten, getoetste voorspellingen, suggesties, backups. Het model schrijft het verhaal;
//! zonder model een vaste opmaak. Ook als herinnering (Summary) + event → melding in de GUI.
// VX:W15.06

use rusqlite::params;
use serde_json::{Value, json};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, TraceId};

use crate::api::AppState;

fn q(log: &EventLog, sql: &str, since: &str) -> Vec<Value> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(sql)?;
            let n = st.column_count();
            let names: Vec<String> = (0..n).map(|i| st.column_name(i).map(String::from).unwrap_or_default()).collect();
            let v = st
                .query_map([since], |r| {
                    let mut o = serde_json::Map::new();
                    for (i, k) in names.iter().enumerate() {
                        let v = match r.get_ref(i)? {
                            rusqlite::types::ValueRef::Integer(x) => json!(x),
                            rusqlite::types::ValueRef::Real(x) => json!(x),
                            rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                            _ => Value::Null,
                        };
                        o.insert(k.clone(), v);
                    }
                    Ok(Value::Object(o))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

/// Alle gegevens voor de briefing (laatste 24 uur).
pub fn gather(state: &AppState) -> Value {
    let log = state.kernel.events();
    let since = (vortex_types::now() - chrono::Duration::hours(24)).to_rfc3339();
    json!({
        "world": q(log, "SELECT title, status, n_items, theme, region FROM world_events WHERE last_seen >= ?1 ORDER BY status = 'confirmed' DESC, n_items DESC LIMIT 8", &since),
        "learned": {
            "by_kind": q(log, "SELECT kind, COUNT(*) AS n FROM memories WHERE created_at >= ?1 AND stale = 0 GROUP BY kind ORDER BY n DESC", &since),
            "examples": q(log, "SELECT title, kind FROM memories WHERE created_at >= ?1 AND stale = 0 AND title != '' AND kind NOT IN ('short_term') ORDER BY importance DESC LIMIT 6", &since),
            "layers_promoted": q(log, "SELECT json_extract(payload, '$.layer') AS layer FROM events WHERE kind = 'neural.layer.promoted' AND ts >= ?1", &since),
            "adapters": q(log, "SELECT id, status FROM adapters WHERE updated_at >= ?1", &since),
            "agenda_done": q(log, "SELECT topic, reason FROM learning_agenda WHERE status = 'done' AND updated_at >= ?1 LIMIT 8", &since),
        },
        "conflicts": q(log, "SELECT COUNT(*) AS open FROM conflicts WHERE status = 'open' AND ?1 = ?1", &since),
        "tasks": q(log, "SELECT status, COUNT(*) AS n FROM tasks WHERE updated_at >= ?1 GROUP BY status", &since),
        "failed_tasks": q(log, "SELECT title, error FROM tasks WHERE status = 'failed' AND updated_at >= ?1 LIMIT 5", &since),
        "projects": q(log, "SELECT p.name, p.status, (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.status = 'done') AS done, (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id) AS total FROM projects p WHERE p.status = 'active' AND ?1 = ?1", &since),
        "predictions_resolved": q(log, "SELECT claim, probability, status FROM predictions WHERE resolved_at >= ?1", &since),
        "predictions_due_soon": q(log, "SELECT claim, resolve_by FROM predictions WHERE status = 'open' AND resolve_by <= datetime(?1, '+3 days') ORDER BY resolve_by LIMIT 5", &vortex_types::now().to_rfc3339()),
        "suggestions": q(log, "SELECT text FROM suggestions WHERE status = 'new' AND ?1 = ?1 ORDER BY created_at DESC LIMIT 5", &since),
        "backup": q(log, "SELECT created_at, verified, bytes FROM backups WHERE ?1 = ?1 ORDER BY created_at DESC LIMIT 1", &since),
    })
}

/// Vaste opmaak (ook als terugval zonder model).
pub fn render(d: &Value) -> String {
    let mut md = String::new();
    let list = |v: &Value, f: &dyn Fn(&Value) -> String| {
        v.as_array().map(|a| a.iter().map(|x| format!("- {}\n", f(x))).collect::<String>()).unwrap_or_default()
    };
    md.push_str("## Wereld\n");
    let w = list(&d["world"], &|x| {
        format!(
            "{} ({}, {} bronnen{})",
            x["title"].as_str().unwrap_or(""),
            if x["status"] == "confirmed" { "bevestigd" } else { "gemeld" },
            x["n_items"],
            x["theme"].as_str().map(|t| format!(", {t}")).unwrap_or_default()
        )
    });
    md.push_str(if w.is_empty() { "- (geen nieuwe events)\n" } else { &w });
    md.push_str("\n## Wat Vortex leerde\n");
    let k = d["learned"]["by_kind"]
        .as_array()
        .map(|a| {
            a.iter().map(|x| format!("{} {}", x["n"], x["kind"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(", ")
        })
        .unwrap_or_default();
    md.push_str(&format!("- Nieuwe herinneringen: {}\n", if k.is_empty() { "geen".into() } else { k }));
    md.push_str(&list(&d["learned"]["examples"], &|x| x["title"].as_str().unwrap_or("").to_string()));
    md.push_str(&list(&d["learned"]["layers_promoted"], &|x| {
        format!("Neurale layer gepromoveerd: {}", x["layer"].as_str().unwrap_or(""))
    }));
    md.push_str(&list(&d["learned"]["agenda_done"], &|x| {
        format!("Leeragenda: {} ({})", x["topic"].as_str().unwrap_or(""), x["reason"].as_str().unwrap_or(""))
    }));
    md.push_str(&format!("\n## Open conflicten\n- {}\n", d["conflicts"][0]["open"].as_i64().unwrap_or(0)));
    md.push_str("\n## Taken en projecten\n");
    md.push_str(&list(&d["tasks"], &|x| format!("{}: {}", x["status"].as_str().unwrap_or(""), x["n"])));
    md.push_str(&list(&d["failed_tasks"], &|x| {
        format!("Mislukt: {} — {}", x["title"].as_str().unwrap_or(""), x["error"].as_str().unwrap_or(""))
    }));
    md.push_str(&list(&d["projects"], &|x| {
        format!("Project {}: {}/{} taken klaar", x["name"].as_str().unwrap_or(""), x["done"], x["total"])
    }));
    let pr = list(&d["predictions_resolved"], &|x| {
        format!(
            "{} ({:.0}%) → {}",
            x["claim"].as_str().unwrap_or(""),
            x["probability"].as_f64().unwrap_or(0.0) * 100.0,
            x["status"].as_str().unwrap_or("")
        )
    });
    if !pr.is_empty() {
        md.push_str("\n## Getoetste voorspellingen\n");
        md.push_str(&pr);
    }
    let sg = list(&d["suggestions"], &|x| x["text"].as_str().unwrap_or("").to_string());
    if !sg.is_empty() {
        md.push_str("\n## Suggesties\n");
        md.push_str(&sg);
    }
    if let Some(b) = d["backup"].as_array().and_then(|a| a.first()) {
        md.push_str(&format!(
            "\n## Backup\n- Laatste: {} ({})\n",
            b["created_at"].as_str().unwrap_or("").get(..16).unwrap_or(""),
            if b["verified"] == 1 { "restore-test geslaagd" } else { "niet geverifieerd" }
        ));
    }
    md
}

/// Briefing schrijven voor `day` (standaard vandaag); bestaat hij al, dan wordt hij vervangen.
pub async fn write(state: &AppState, day: Option<String>) -> Result<Value, String> {
    let day = day.unwrap_or_else(|| chrono::Local::now().date_naive().to_string());
    let data = gather(state);
    let base = render(&data);
    let cfg = state.kernel.config().current();
    let llm = state.chat.role_llm(&cfg, "chat", state.kernel.shutdown_token().child_token());
    let mut md = base.clone();
    if let Some(llm) = llm {
        let out = llm
            .complete(
                vec![
                    vortex_models::backend::Message::system(
                        "Schrijf de dagelijkse briefing voor de gebruiker in het Nederlands: kort, helder, met kopjes (Wereld, Wat ik leerde, Aandacht nodig, Taken). Alleen wat in de gegevens staat; bevestigd en alleen-gemeld nieuws duidelijk onderscheiden. Markdown.",
                    ),
                    vortex_models::backend::Message::user(format!("Datum: {day}\nGegevens (JSON):\n{data}\n\nStandaardopmaak:\n{base}")),
                ],
                vortex_cognition::GenOpts { max_tokens: Some(900), temperature: Some(0.3), json_schema: None, reasoning_effort: None },
            )
            .await;
        if let Ok(t) = out
            && t.trim().len() > 80
        {
            md = t.trim().to_string();
        }
    }
    let md = format!("# Briefing {day}\n\n{md}");
    let log = state.kernel.events();
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO briefings(day, markdown, data, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(day) DO UPDATE SET markdown = ?2, data = ?3, created_at = ?4",
                params![day, md, data.to_string(), vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    if let Some(mem) = &state.chat.memory {
        let mut m = vortex_memory::NewMemory::new(
            vortex_memory::MemoryKind::Summary,
            md.clone(),
            vortex_types::Provenance::with_origin(vortex_types::SourceKind::System, "briefing"),
        );
        m.title = format!("Briefing {day}");
        m.source_hash = Some(format!("briefing:{day}"));
        m.tags = vec!["briefing".into()];
        m.force = true;
        let _ = mem.store.write_or_defer(m).await;
    }
    let _ = log.append(Event::new(EventKind::BriefingWritten, TraceId::new(), json!({"day": day, "chars": md.len()})));
    Ok(json!({"day": day, "markdown": md, "summary": format!("briefing {day}")}))
}

pub fn list(log: &EventLog, limit: usize) -> Vec<Value> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT day, markdown, created_at FROM briefings ORDER BY day DESC LIMIT ?1")?;
            let v = st
                .query_map([limit as i64], |r| Ok(json!({"day": r.get::<_, String>(0)?, "markdown": r.get::<_, String>(1)?, "created_at": r.get::<_, String>(2)?})))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}
