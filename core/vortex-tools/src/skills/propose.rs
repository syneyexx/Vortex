//! Skill-voorstellen uit geslaagde traces (W08.14, basis): een geslaagde run met ≥ 2
//! toolstappen wordt een voorstel (stappen als recept). Zelfde stappenpatroon opnieuw →
//! teller omhoog. De gebruiker accepteert (→ skill) of wijst af.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventId, EventKind, TraceId};

use super::Skill;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub seen: i64,
    pub status: String,
    pub source_run: Option<String>,
}

/// Eén geslaagde toolstap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub tool: String,
    pub input: serde_json::Value,
}

fn name_from(question: &str) -> String {
    let words: Vec<&str> = question.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 3).take(4).collect();
    let n = Skill::slug(&words.join(" "));
    if n.is_empty() { "recept".into() } else { n }
}

/// Bekijk een afgeronde run. Geeft het voorstel-ID als er een voorstel is gemaakt/versterkt.
pub fn observe(log: &EventLog, question: &str, steps: &[Step], success: bool, run: Option<String>) -> Option<String> {
    if !success || steps.len() < 2 {
        return None;
    }
    let sig: String = steps.iter().map(|s| s.tool.as_str()).collect::<Vec<_>>().join(">");
    let name = name_from(question);
    let description = format!("Recept voor taken zoals: {}", question.chars().take(160).collect::<String>());
    let mut body = format!("# {name}\n\nGeleerd uit een geslaagde taak. Stappen:\n\n");
    for (k, s) in steps.iter().enumerate() {
        let inp = serde_json::to_string(&s.input).unwrap_or_default();
        body.push_str(&format!(
            "{}. `{}` — invoer zoals {}\n",
            k + 1,
            s.tool,
            inp.chars().take(200).collect::<String>()
        ));
    }
    body.push_str("\nPas paden/waarden aan de nieuwe taak aan; controleer het resultaat na de laatste stap.\n");
    let now = vortex_types::now().to_rfc3339();
    let id = EventId::new().to_string();
    let r = log.transact(|tx, out| {
        tx.execute(
            "INSERT INTO skill_proposals(id, name, signature, description, body, source_run, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'proposed', ?7, ?7)
             ON CONFLICT(signature) DO UPDATE SET seen = seen + 1, updated_at = ?7",
            params![id, name, sig, description, body, run, now],
        )?;
        let pid: String = tx.query_row("SELECT id FROM skill_proposals WHERE signature = ?1", [&sig], |r| r.get(0))?;
        out.emit(tx, Event::new(EventKind::SkillProposed, TraceId::new(), serde_json::json!({"id": pid, "name": name, "steps": steps.len()})))?;
        Ok(pid)
    });
    r.ok()
}

pub fn list(log: &EventLog, status: Option<&str>) -> Vec<Proposal> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(
                "SELECT id, name, description, body, seen, status, source_run FROM skill_proposals
                 WHERE (?1 IS NULL OR status = ?1) ORDER BY seen DESC, updated_at DESC LIMIT 200",
            )?;
            let v = st
                .query_map([status], |r| {
                    Ok(Proposal {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        body: r.get(3)?,
                        seen: r.get(4)?,
                        status: r.get(5)?,
                        source_run: r.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

pub fn set_status(log: &EventLog, id: &str, status: &str) -> Result<Option<Proposal>, String> {
    let now = vortex_types::now().to_rfc3339();
    log.db()
        .write(|tx| {
            tx.execute(
                "UPDATE skill_proposals SET status = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, status, now],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(list(log, None).into_iter().find(|p| p.id == id))
}

/// Voorstel → skill.
pub fn to_skill(p: &Proposal) -> Skill {
    Skill {
        name: p.name.clone(),
        description: p.description.clone(),
        body: p.body.clone(),
        origin: "proposed".into(),
        version: "1".into(),
        ..Default::default()
    }
}
