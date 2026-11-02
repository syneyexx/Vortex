//! Automatische backups (W15.10): online SQLite-backup + config naar de koude schijf, bewaren
//! (7 dagelijks, 4 wekelijks, 12 maandelijks), en elke keer een restore-test: de kopie openen,
//! `integrity_check`, schemaversie en tellingen van de belangrijkste tabellen vergelijken.
// VX:W15.10

use std::path::{Path, PathBuf};

use rusqlite::{OpenFlags, params};
use serde_json::{Value, json};
use vortex_types::{Event, EventKind, TraceId};

use crate::api::AppState;

const TABLES: &[&str] = &["events", "sessions", "messages", "memories", "entities", "edges", "tasks", "world_events"];

pub fn backup_dir(state: &AppState) -> PathBuf {
    crate::research_api::cold_dir(&state.kernel.config().current(), &state.chat.data_dir()).join("backups")
}

fn counts(c: &rusqlite::Connection) -> serde_json::Map<String, Value> {
    TABLES
        .iter()
        .filter_map(|t| {
            c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0))
                .ok()
                .map(|n| (t.to_string(), json!(n)))
        })
        .collect()
}

/// Restore-test: kan de kopie geopend worden, is hij intact, en klopt de inhoud met de bron
/// (tellingen ≤ live, want live loopt door; schemaversie gelijk)?
pub fn verify(path: &Path, live: &rusqlite::Connection) -> Result<Value, String> {
    let c = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    let ic: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if ic != "ok" {
        return Err(format!("integrity_check: {ic}"));
    }
    let v_b: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap_or(-1);
    let v_l: i64 = live.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap_or(-2);
    if v_b != v_l {
        return Err(format!("schemaversie {v_b} ≠ {v_l}"));
    }
    let (cb, cl) = (counts(&c), counts(live));
    for (k, n) in &cb {
        let l = cl.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        if n.as_i64().unwrap_or(0) > l {
            return Err(format!("{k}: backup {n} > live {l}"));
        }
    }
    // Een echte terugzet-proef: kopie in een tijdelijk bestand herstellen en openen.
    let tmp = path.with_extension("restoretest.db");
    vortex_memory::MemoryStore::restore(path, &tmp).map_err(|e| e.to_string())?;
    let ok = rusqlite::Connection::open(&tmp)
        .and_then(|t| t.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0)))
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    Ok(json!({"integrity": ic, "schema": v_b, "counts": cb, "restored_events": ok}))
}

/// Bewaarbeleid: per backup een "laag" (daily/weekly/monthly); oudste buiten het beleid weg.
fn tier_for(now: chrono::DateTime<chrono::Local>) -> &'static str {
    use chrono::Datelike;
    if now.day() == 1 {
        "monthly"
    } else if now.weekday() == chrono::Weekday::Sun {
        "weekly"
    } else {
        "daily"
    }
}

fn prune(state: &AppState) -> usize {
    let log = state.kernel.events();
    let mut removed = 0;
    for (tier, keep) in [("daily", 7), ("weekly", 4), ("monthly", 12)] {
        let old: Vec<(String, String)> = log
            .db()
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT id, path FROM backups WHERE tier = ?1 ORDER BY created_at DESC LIMIT -1 OFFSET ?2",
                )?;
                let v = st
                    .query_map(params![tier, keep], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        for (id, p) in old {
            let _ = std::fs::remove_file(&p);
            let _ = std::fs::remove_dir_all(PathBuf::from(&p).with_extension("config"));
            let _ = log.db().write(|tx| Ok(tx.execute("DELETE FROM backups WHERE id = ?1", [&id])?));
            removed += 1;
        }
    }
    removed
}

pub async fn run(state: &AppState) -> Result<Value, String> {
    let db = state.kernel.db().clone();
    let src = db.path().ok_or("in-memory database: geen backup mogelijk")?.to_path_buf();
    let dir = backup_dir(state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let now = chrono::Local::now();
    let id = format!("bk_{}", now.format("%Y%m%d-%H%M%S"));
    let dest = dir.join(format!("{id}.db"));
    let cfg_dir = state.kernel.config().dir().to_path_buf();
    let dest2 = dest.clone();
    let src2 = src.clone();
    // Zwaar I/O-werk buiten de async-threads.
    let info = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let _ = db.checkpoint_wal();
        let from = rusqlite::Connection::open_with_flags(&src2, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        let mut to = rusqlite::Connection::open(&dest2).map_err(|e| e.to_string())?;
        rusqlite::backup::Backup::new(&from, &mut to)
            .and_then(|b| b.run_to_completion(512, std::time::Duration::from_millis(2), None))
            .map_err(|e| e.to_string())?;
        drop(to);
        // Config mee (klein, maar onmisbaar bij terugzetten).
        let cdest = dest2.with_extension("config");
        copy_dir(&cfg_dir, &cdest).map_err(|e| e.to_string())?;
        verify(&dest2, &from)
    })
    .await
    .map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let (verified, vinfo) = match &info {
        Ok(v) => (1, v.clone()),
        Err(e) => (0, json!({"error": e})),
    };
    let tier = tier_for(now);
    let log = state.kernel.events();
    log.db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO backups(id, path, bytes, tier, verified, verify_info, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![id, dest.display().to_string(), bytes as i64, tier, verified, vinfo.to_string(), vortex_types::now().to_rfc3339()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    let removed = prune(state);
    let out = json!({"id": id, "path": dest.display().to_string(), "bytes": bytes, "tier": tier, "verified": verified == 1, "verify": vinfo, "pruned": removed, "summary": format!("backup {id} ({} MB){}", bytes / 1_000_000, if verified == 1 { ", restore-test ok" } else { ", restore-test MISLUKT" })});
    let _ = log.append(Event::new(EventKind::BackupCompleted, TraceId::new(), out.clone()));
    info.map(|_| out)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let p = e.path();
        let d = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &d)?;
        } else {
            std::fs::copy(&p, &d)?;
        }
    }
    Ok(())
}

pub fn list(state: &AppState) -> Vec<Value> {
    state
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, path, bytes, tier, verified, verify_info, created_at FROM backups ORDER BY created_at DESC")?;
            let v = st
                .query_map([], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "path": r.get::<_, String>(1)?, "bytes": r.get::<_, i64>(2)?, "tier": r.get::<_, String>(3)?, "verified": r.get::<_, i64>(4)? == 1, "verify": serde_json::from_str::<Value>(&r.get::<_, Option<String>>(5)?.unwrap_or_default()).unwrap_or_default(), "created_at": r.get::<_, String>(6)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}
