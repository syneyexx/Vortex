//! Proactieve monitoring (W15.05): kijkt periodiek naar de staat van het systeem en doet
//! suggesties (de gebruiker accepteert → taak). Geen LLM nodig; elke suggestie één keer per dag.
// VX:W15.05

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, TraceId};

use super::queue::NewTask;
use crate::api::AppState;

fn count(log: &EventLog, sql: &str) -> i64 {
    log.db().read(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?)).unwrap_or(0)
}

/// Suggestie toevoegen (dedup per soort + tekst per dag).
pub fn suggest(log: &EventLog, kind: &str, text: &str, action: Option<&NewTask>) -> Option<String> {
    let day = chrono::Local::now().date_naive().to_string();
    let id = format!(
        "sug_{kind}_{}_{:08x}",
        day.replace('-', ""),
        text.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
    );
    let now = vortex_types::now().to_rfc3339();
    let n = log
        .db()
        .write(|tx| {
            Ok(tx.execute(
                "INSERT OR IGNORE INTO suggestions(id, kind, text, action, status, created_at, updated_at) VALUES (?1,?2,?3,?4,'new',?5,?5)",
                params![id, kind, text, action.map(|a| serde_json::to_string(a).unwrap_or_default()), now],
            )?)
        })
        .unwrap_or(0);
    (n > 0).then(|| {
        let _ = log.append(Event::new(
            EventKind::SuggestionCreated,
            TraceId::new(),
            json!({"id": id, "kind": kind, "text": text}),
        ));
        id
    })
}

fn task(title: &str, kind: &str, params: Value) -> NewTask {
    NewTask {
        title: title.into(),
        kind: kind.into(),
        params,
        priority: 2,
        needs_idle: Some(false),
        ..Default::default()
    }
}

/// Eén monitoringronde. Geeft de nieuwe suggesties.
pub async fn scan(state: &AppState) -> Value {
    let log = state.kernel.events();
    let mut new = Vec::new();
    let mut add = |kind: &str, text: String, action: Option<NewTask>| {
        if let Some(id) = suggest(log, kind, &text, action.as_ref()) {
            new.push(json!({"id": id, "kind": kind, "text": text}));
        }
    };
    // Open conflicten.
    let conflicts = count(log, "SELECT COUNT(*) FROM conflicts WHERE status = 'open'");
    if conflicts >= 3 {
        add(
            "conflicts",
            format!("Er staan {conflicts} geheugenconflicten open. Zal ik ze laten beoordelen door het conflict-team?"),
            None,
        );
    }
    // Mislukte taken in de laatste dag.
    let failed =
        count(log, "SELECT COUNT(*) FROM tasks WHERE status = 'failed' AND updated_at >= datetime('now', '-1 day')");
    if failed > 0 {
        add("tasks", format!("{failed} taak/taken mislukten het afgelopen etmaal — bekijk de taakqueue."), None);
    }
    // Backup verouderd of restore-test mislukt.
    let last: Option<(String, i64)> = log
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT created_at, verified FROM backups ORDER BY created_at DESC LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
        })
        .ok()
        .flatten();
    match last {
        None => add(
            "backup",
            "Er is nog geen backup gemaakt. Nu een backup maken?".into(),
            Some(task("Backup maken", "backup", json!({}))),
        ),
        Some((t, v)) => {
            let old = chrono::DateTime::parse_from_rfc3339(&t)
                .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_hours() > 48)
                .unwrap_or(true);
            if old {
                add(
                    "backup",
                    "De laatste backup is ouder dan 2 dagen. Nu een backup maken?".into(),
                    Some(task("Backup maken", "backup", json!({}))),
                );
            }
            if v == 0 {
                add(
                    "backup",
                    "De restore-test van de laatste backup is mislukt. Opnieuw proberen?".into(),
                    Some(task("Backup + restore-test", "backup", json!({}))),
                );
            }
        }
    }
    // Leeragenda loopt vol.
    let agenda = count(log, "SELECT COUNT(*) FROM learning_agenda WHERE status = 'open'");
    if agenda >= 10 {
        add(
            "agenda",
            format!("De leeragenda heeft {agenda} open onderwerpen. Nu een paar laten onderzoeken?"),
            Some(task("Leeragenda uitvoeren", "agenda", json!({"max": 3}))),
        );
    }
    // Wereld: bevestigde events in thema's waar de gebruiker iets mee heeft (gebruikers-/projectgeheugen).
    let interests: Vec<String> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT DISTINCT e.norm FROM entities e JOIN memory_entities me ON me.entity_id = e.id JOIN memories m ON m.id = me.memory_id WHERE m.kind IN ('user','project') AND m.stale = 0 AND length(e.norm) >= 4 LIMIT 200")?;
            let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    if !interests.is_empty() {
        let evs: Vec<(String, String)> = log
            .db()
            .read(|c| {
                let mut st = c.prepare("SELECT id, title FROM world_events WHERE status = 'confirmed' AND last_seen >= datetime('now', '-1 day') ORDER BY n_items DESC LIMIT 50")?;
                let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        for (id, title) in evs {
            let tl = title.to_lowercase();
            if let Some(hit) = interests.iter().find(|i| tl.contains(i.as_str())) {
                add(
                    "world",
                    format!("Nieuws dat raakt aan \"{hit}\": {title}"),
                    Some(task(
                        &format!("Uitzoeken: {title}"),
                        "research",
                        json!({"question": format!("Wat betekent dit voor mij: {title}?"), "mode": "quick", "event": id}),
                    )),
                );
            }
        }
    }
    // Voorspellingen die binnenkort getoetst worden.
    let due = count(log, "SELECT COUNT(*) FROM predictions WHERE status = 'open' AND resolve_by <= datetime('now')");
    if due > 0 {
        add(
            "predictions",
            format!("{due} voorspelling(en) zijn over hun deadline en kunnen getoetst worden."),
            Some(task("Wereldmonitor-cyclus (toetsen)", "world", json!({}))),
        );
    }
    // Schijfruimte op de koude schijf.
    let dir = super::backup::backup_dir(state);
    if let Some(free) = free_bytes(dir.parent().unwrap_or(&dir))
        && free < 20_000_000_000
    {
        add("disk", format!("Nog maar {} GB vrij op de koude schijf.", free / 1_000_000_000), None);
    }
    json!({"new": new, "summary": format!("{} nieuwe suggestie(s)", new.len())})
}

fn free_bytes(p: &std::path::Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let p = std::fs::canonicalize(p).ok().or_else(|| std::fs::canonicalize(".").ok())?;
    disks
        .list()
        .iter()
        .filter(|d| p.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

pub fn list(log: &EventLog, status: Option<&str>) -> Vec<Value> {
    log.db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, kind, text, action, status, created_at FROM suggestions WHERE (?1 IS NULL OR status = ?1) ORDER BY created_at DESC LIMIT 100")?;
            let v = st
                .query_map([status], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "text": r.get::<_, String>(2)?, "action": r.get::<_, Option<String>>(3)?.and_then(|a| serde_json::from_str::<Value>(&a).ok()), "status": r.get::<_, String>(4)?, "created_at": r.get::<_, String>(5)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

/// Suggestie accepteren (→ taak in de queue) of wegklikken.
pub fn decide(state: &AppState, id: &str, accept: bool) -> Result<Value, String> {
    let log = state.kernel.events();
    let action: Option<String> = log
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT action FROM suggestions WHERE id = ?1 AND status = 'new'", [id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten())
        })
        .map_err(|e| e.to_string())?;
    let _ = log.db().write(|tx| {
        tx.execute(
            "UPDATE suggestions SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, if accept { "accepted" } else { "dismissed" }, vortex_types::now().to_rfc3339()],
        )?;
        Ok(())
    });
    if accept && let Some(a) = action.and_then(|a| serde_json::from_str::<NewTask>(&a).ok()) {
        let t = state.autonomy.queue.enqueue(NewTask { origin: Some(format!("suggestion:{id}")), ..a })?;
        return Ok(json!({"task": t}));
    }
    Ok(json!({"ok": true}))
}
