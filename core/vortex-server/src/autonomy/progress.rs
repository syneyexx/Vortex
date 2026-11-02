//! Voortgangsdashboard "hoe slim is Vortex" (W15.09): eval-scores per maand per suite, groei
//! van het geheugen, gepromoveerde layers, adapters, kalibratie van het wereldmodel, afgehandelde
//! leeragenda. Een suite die een maand achteruitgaat → leeragenda (`weak_task`) → training.
// VX:W15.09

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vortex_core::EventLog;

fn rows2(log: &EventLog, sql: &str) -> Vec<(String, String, f64)> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(sql)?;
            let v = st
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<f64>>(2)?.unwrap_or(0.0)))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

pub fn compute(log: &EventLog) -> Value {
    // Eval: beste score per suite per maand (labels "modus …" tellen niet mee als baseline-runs).
    let eval = rows2(
        log,
        "SELECT substr(started_at, 1, 7) AS m, suite, AVG(score) FROM eval_runs WHERE score IS NOT NULL AND finished_at IS NOT NULL GROUP BY m, suite ORDER BY m",
    );
    let mut by_suite: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for (m, s, v) in &eval {
        by_suite.entry(s.clone()).or_default().insert(m.clone(), *v);
    }
    let mut overall: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for (m, _, v) in &eval {
        let e = overall.entry(m.clone()).or_default();
        e.0 += v;
        e.1 += 1;
    }
    let trend: Vec<Value> = by_suite
        .iter()
        .filter_map(|(s, months)| {
            let v: Vec<f64> = months.values().copied().collect();
            (v.len() >= 2).then(|| json!({"suite": s, "last": v[v.len() - 1], "previous": v[v.len() - 2], "delta": v[v.len() - 1] - v[v.len() - 2]}))
        })
        .collect();
    let memory = rows2(
        log,
        "SELECT substr(created_at, 1, 7) AS m, kind, COUNT(*) FROM memories WHERE stale = 0 GROUP BY m, kind ORDER BY m",
    );
    let mut mem: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for (m, k, n) in memory {
        mem.entry(m).or_default().insert(k, n);
    }
    let count = |sql: &str| log.db().read(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?)).unwrap_or(0);
    json!({
        "eval_by_suite": by_suite,
        "eval_overall": overall.iter().map(|(m, (s, n))| json!({"month": m, "score": s / *n as f64})).collect::<Vec<_>>(),
        "trend": trend,
        "memory_by_month": mem,
        "layers_promoted": count("SELECT COUNT(*) FROM events WHERE kind = 'neural.layer.promoted'"),
        "adapters_active": count("SELECT COUNT(*) FROM adapters WHERE status = 'active'"),
        "agenda_done": count("SELECT COUNT(*) FROM learning_agenda WHERE status = 'done'"),
        "agenda_open": count("SELECT COUNT(*) FROM learning_agenda WHERE status = 'open'"),
        "tasks_done": count("SELECT COUNT(*) FROM tasks WHERE status = 'done'"),
        "world_calibration": vortex_agents::world::calibration(log)["overall"],
    })
}

/// Achteruitgang (> 5 punten t.o.v. vorige maand) → leeragenda + vliegwiel pakt het op.
pub fn steer(log: &EventLog, agenda: &vortex_cognition::Agenda) -> usize {
    let p = compute(log);
    let mut n = 0;
    for t in p["trend"].as_array().into_iter().flatten() {
        if t["delta"].as_f64().unwrap_or(0.0) < -0.05
            && let Some(s) = t["suite"].as_str()
        {
            let _ = agenda.add(
                s,
                "weak_task",
                &format!("eval-suite {s} ging {:.0} punten achteruit", t["delta"].as_f64().unwrap_or(0.0) * -100.0),
                0.7,
            );
            n += 1;
        }
    }
    n
}
