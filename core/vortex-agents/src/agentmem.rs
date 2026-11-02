//! Multi-agent geheugen (W18.07): elke rol heeft een privé-werkruimte (`agent:<rol>:<run>`),
//! het team een gedeelde (`team:<run>`). Na het debat: kandidaten → privé, gestaafde beweringen
//! → gedeeld; alleen als de Judge akkoord geeft worden gedeelde feiten algemene kennis.
// VX:W18.07

use serde_json::{Value, json};
use vortex_memory::{Filters, MemoryKind, MemoryStore, NewMemory, Query};
use vortex_types::{Provenance, SourceKind};

use crate::debate::DebateOutcome;
use crate::evidence::Label;

pub fn private_ws(run: &str, role: &str) -> String {
    format!("agent:{role}:{run}")
}

pub fn shared_ws(run: &str) -> String {
    format!("team:{run}")
}

async fn put(mem: &MemoryStore, kind: MemoryKind, ws: Option<String>, text: &str, origin: &str, conf: f64) -> bool {
    let mut m = NewMemory::new(kind, text, Provenance::with_origin(SourceKind::Model, origin));
    m.workspace = ws;
    m.confidence = Some(conf);
    m.force = true;
    mem.write_or_defer(m).await.is_ok()
}

/// Debatuitkomst vastleggen. Geeft tellingen (privé, gedeeld, gepromoveerd).
pub async fn record(mem: &MemoryStore, run: &str, out: &DebateOutcome) -> Value {
    let (mut private, mut shared, mut promoted) = (0, 0, 0);
    for (id, text, score, chosen) in &out.candidates {
        if put(
            mem,
            MemoryKind::Episodic,
            Some(private_ws(run, &format!("solver-{id}"))),
            &text.chars().take(4000).collect::<String>(),
            &format!("solver {id}{}", if *chosen { " (gekozen)" } else { "" }),
            *score,
        )
        .await
        {
            private += 1;
        }
    }
    let accept = out.trusted && out.verdict.verdict == "accept";
    for c in out.evidence.iter().filter(|c| c.label == Label::Supported) {
        if put(mem, MemoryKind::Semantic, Some(shared_ws(run)), &c.claim, "fact-checker", 0.8).await {
            shared += 1;
        }
        if accept
            && put(
                mem,
                MemoryKind::Semantic,
                None,
                &c.claim,
                "debatteam (Judge akkoord)",
                (0.6 + 0.3 * out.verdict.confidence).min(0.95),
            )
            .await
        {
            promoted += 1;
        }
    }
    json!({"private": private, "shared": shared, "promoted": promoted})
}

/// Wat een rol ziet: eigen privé-notities + het gedeelde teamgeheugen (+ algemene kennis).
pub async fn recall(mem: &MemoryStore, run: &str, role: &str, query: &str, k: usize) -> Vec<String> {
    let mut out = Vec::new();
    for ws in [private_ws(run, role), shared_ws(run)] {
        let mut q = Query::new(query);
        q.k = k;
        q.filters = Filters { workspace: Some(ws), ..Default::default() };
        for r in mem.retrieve(&q, None).await.unwrap_or_default() {
            if !out.contains(&r.memory.content) {
                out.push(r.memory.content.clone());
            }
        }
    }
    out
}
