//! W18.07: privé per rol, gedeeld per team, promotie alleen na Judge-akkoord.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use vortex_agents::agentmem::{recall, record};
use vortex_agents::debate::{DebateOutcome, Verdict};
use vortex_agents::evidence::{ClaimCheck, Label};
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog};
use vortex_memory::{Filters, HashEmbedder, HeuristicLlm, MemConfig, MemoryStore};

fn outcome(accept: bool) -> DebateOutcome {
    let claim = |c: &str, l: Label| ClaimCheck {
        claim: c.into(),
        label: l,
        why: String::new(),
        sources: vec![],
        scope: "lokaal".into(),
    };
    DebateOutcome {
        draft: "concept".into(),
        trusted: accept,
        verdict: Verdict {
            verdict: if accept { "accept" } else { "revise" }.into(),
            confidence: 0.9,
            reasons: vec![],
            fix: String::new(),
        },
        evidence: vec![
            claim("De Eiffeltoren staat in Parijs", Label::Supported),
            claim("De maan is van kaas", Label::Contradicted),
        ],
        candidates: vec![
            ("a".into(), "Kandidaat A: Parijs, Frankrijk".into(), 0.9, true),
            ("b".into(), "Kandidaat B: Lyon".into(), 0.2, false),
        ],
        rounds: 1,
        scope: "lokaal".into(),
        plan: vec![],
    }
}

#[tokio::test]
async fn private_shared_and_promotion() {
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    let mem = MemoryStore::open(
        log,
        Arc::new(HashEmbedder::new(256)),
        Arc::new(HeuristicLlm),
        MemConfig { dedup_threshold: 0.999, ..Default::default() },
    )
    .unwrap();
    let r = record(&mem, "run1", &outcome(false)).await;
    assert_eq!(
        (r["private"].as_i64(), r["shared"].as_i64(), r["promoted"].as_i64()),
        (Some(2), Some(1), Some(0)),
        "{r}"
    );
    // Solver a ziet eigen notitie + gedeelde feiten, niet die van solver b.
    let seen = recall(&mem, "run1", "solver-a", "waar staat de toren kandidaat", 5).await;
    assert!(
        seen.iter().any(|t| t.contains("Kandidaat A")) && !seen.iter().any(|t| t.contains("Kandidaat B")),
        "{seen:?}"
    );
    assert!(seen.iter().any(|t| t.contains("Eiffeltoren")));
    let general = |m: &MemoryStore| {
        m.list(&Filters::default(), None, 100, 0).unwrap().into_iter().filter(|x| x.workspace.is_none()).count()
    };
    assert_eq!(general(&mem), 0, "zonder Judge-akkoord niets algemeen");
    let r2 = record(&mem, "run2", &outcome(true)).await;
    assert_eq!(r2["promoted"].as_i64(), Some(1), "alleen de gestaafde bewering");
    assert_eq!(general(&mem), 1);
}
