//! W12: temporele graaf + redeneren, dromen (gate: retrieval wordt beter), tiering (koud ⇄ warm),
//! meta-memory, procedures en few-shot.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog};
use vortex_memory::dream::DreamOptions;
use vortex_memory::graph::default_rules;
use vortex_memory::procedural::Procedure;
use vortex_memory::tiering::TierConfig;
use vortex_memory::{HashEmbedder, HeuristicLlm, MemConfig, MemoryKind, MemoryStore, NewMemory, Query, WriteOutcome};
use vortex_types::{MemoryId, Provenance, SourceKind};

fn store() -> (Arc<MemoryStore>, EventLog) {
    let l = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    // Hoge schrijf-dedupdrempel: testteksten blijven apart, dromen voegt ze later samen.
    let cfg = MemConfig { dedup_threshold: 0.995, ..MemConfig::default() };
    (Arc::new(MemoryStore::open(l.clone(), Arc::new(HashEmbedder::new(256)), Arc::new(HeuristicLlm), cfg).unwrap()), l)
}

fn lc(s: &str) -> String {
    s.to_lowercase()
}

async fn put(s: &MemoryStore, kind: MemoryKind, text: &str, vf: Option<&str>, vt: Option<&str>) -> MemoryId {
    let mut nm = NewMemory::new(kind, text, Provenance::with_origin(SourceKind::User, "test"));
    nm.force = true;
    nm.valid_from = vf.map(String::from);
    nm.valid_to = vt.map(String::from);
    match s.write(nm).await.unwrap() {
        WriteOutcome::Stored { ids, .. } => ids[0],
        WriteOutcome::Merged { into } => into,
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn temporal_graph_and_reasoning() {
    let (s, l) = store();
    put(&s, MemoryKind::User, "Syneye woont in Utrecht.", Some("2020-01-01"), Some("2025-06-01")).await;
    put(&s, MemoryKind::User, "Syneye woont in Amsterdam.", Some("2025-06-01"), None).await;
    put(&s, MemoryKind::Semantic, "Utrecht ligt in Nederland.", None, None).await;
    put(&s, MemoryKind::Semantic, "Nederland ligt in Europa.", None, None).await;
    // Op een datum: wat gold toen?
    let then = s.facts_about("syneye", Some("2023-03-01")).unwrap();
    assert!(then.iter().any(|f| f.rel == "woont in" && lc(&f.dst) == "utrecht"), "{then:?}");
    assert!(!then.iter().any(|f| lc(&f.dst) == "amsterdam"));
    let now = s.facts_about("syneye", None).unwrap();
    assert!(now.iter().any(|f| lc(&f.dst) == "amsterdam") && !now.iter().any(|f| lc(&f.dst) == "utrecht"), "{now:?}");
    // Historie: beide, in volgorde.
    let h = s.fact_history("syneye", Some("woont in")).unwrap();
    assert_eq!(h.iter().map(|f| lc(&f.dst)).collect::<Vec<_>>(), vec!["utrecht", "amsterdam"]);
    // Redeneren: transitief (utrecht ligt in europa) + keten (woont in regio), met steun.
    let rep = s.reason(&default_rules()).unwrap();
    assert!(rep.derived >= 2, "{rep:?}");
    let u = s.facts_about("utrecht", None).unwrap();
    let d = u.iter().find(|f| f.rel == "ligt in" && lc(&f.dst) == "europa").expect("afgeleid");
    assert!(d.derived && d.rule.as_deref().unwrap_or("").contains("transitief"));
    let sup: i64 = l
        .db()
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM edge_support WHERE derived_edge = ?1", [&d.id], |r| r.get(0))?))
        .unwrap();
    assert_eq!(sup, 2);
    // Geen conflict: de twee woonplaatsen overlappen niet in tijd.
    assert_eq!(rep.conflicts_found, 0);
    // Functionele relatie met twee waarden tegelijk (zonder claim-detectie bij schrijven) →
    // consistentieconflict uit het redeneren.
    let a = put(&s, MemoryKind::Semantic, "Notitie over de geboorte van Bram.", None, None).await;
    let b = put(&s, MemoryKind::Semantic, "Andere notitie over waar Bram vandaan komt.", None, None).await;
    l.db()
        .write(|tx| {
            for (id, n) in [("ent_bram", "bram"), ("ent_gouda", "gouda"), ("ent_delft", "delft")] {
                tx.execute("INSERT OR IGNORE INTO entities(id, name, norm) VALUES (?1, ?2, ?2)", rusqlite::params![id, n])?;
            }
            tx.execute("INSERT INTO edges(id, src, rel, dst, memory_id, created_at) VALUES ('e1', 'ent_bram', 'geboren in', 'ent_gouda', ?1, '2026-01-01')", [a.to_string()])?;
            tx.execute("INSERT INTO edges(id, src, rel, dst, memory_id, created_at) VALUES ('e2', 'ent_bram', 'geboren in', 'ent_delft', ?1, '2026-01-01')", [b.to_string()])?;
            Ok(())
        })
        .unwrap();
    let rep = s.reason(&default_rules()).unwrap();
    assert_eq!(rep.conflicts_found, 1, "{rep:?}");
    assert!(s.conflicts(Some("open")).unwrap().iter().any(|c| c.reason.contains("geboren in")));
    // Afgeleide feiten verdwijnen met hun bron (opnieuw berekend).
    let base = s.list(&Default::default(), Some("Nederland ligt in Europa"), 5, 0).unwrap();
    s.forget(base[0].id).unwrap();
    s.reason(&default_rules()).unwrap();
    assert!(!s.facts_about("utrecht", None).unwrap().iter().any(|f| lc(&f.dst) == "europa"));
}

async fn recall_at(s: &MemoryStore, k: usize) -> f64 {
    {
        let qs = [
            ("Wat is de hoofdstad van Kenia?", "Nairobi"),
            ("Welke taal spreekt men in Brazilië?", "Portugees"),
            ("Hoe hoog is de Mount Everest?", "8849"),
        ];
        let mut hit = 0.0;
        for (q, want) in qs {
            let mut qq = Query::new(q.to_string());
            qq.k = k;
            let r = s.retrieve(&qq, None).await.unwrap();
            // Mean reciprocal rank van het juiste antwoord + aandeel unieke (niet-dubbele) resultaten.
            if let Some(pos) = r.iter().position(|i| i.memory.content.contains(want)) {
                hit += 1.0 / (pos as f64 + 1.0);
            }
            let noise = r.iter().filter(|i| i.memory.content.starts_with("Vraag over")).count();
            hit += 0.1 * (1.0 - noise as f64 / r.len().max(1) as f64);
        }
        hit / 3.0
    }
}

#[tokio::test]
async fn gate_dreaming_improves_retrieval() {
    let (s, _) = store();
    put(&s, MemoryKind::Semantic, "De hoofdstad van Kenia is Nairobi.", None, None).await;
    put(&s, MemoryKind::Semantic, "In Brazilië spreekt men Portugees.", None, None).await;
    put(&s, MemoryKind::Semantic, "De Mount Everest is 8849 meter hoog.", None, None).await;
    // Ruis: veel bijna-duplicaten die de vragen woordelijk raken maar het antwoord niet bevatten.
    for suffix in [
        "genoteerd op maandag door Anna",
        "opgeschreven tijdens de vergadering",
        "uit het oude kladblok van Piet",
        "gevonden in een mail van Kees",
        "gekrabbeld op een bierviltje",
        "doorgestuurd door de buurman",
    ] {
        put(&s, MemoryKind::Semantic, &format!("Vraag over de hoofdstad van Kenia en de taal die men spreekt in Brazilië en hoe hoog de Mount Everest is, {suffix}."), None, None).await;
    }
    let before = recall_at(&s, 5).await;
    let rep = s.dream(&DreamOptions { merge_threshold: 0.75, reflect: false, ..Default::default() }).await.unwrap();
    assert!(rep.merged >= 4, "{rep:?}");
    let after = recall_at(&s, 5).await;
    assert!(after > before, "dromen moet retrieval verbeteren: voor {before} na {after}");
    assert_eq!(s.dream_runs(5).unwrap().len(), 1);
}

#[tokio::test]
async fn tiering_cold_and_back() {
    let (s, _) = store();
    let d = tempfile::tempdir().unwrap();
    let mut ids = Vec::new();
    for i in 0..300 {
        ids.push(
            put(
                &s,
                MemoryKind::Semantic,
                &format!(
                    "Archieffeit nummer {i}: de sensor in kast {} mat {} graden op dag {}.",
                    i % 17,
                    20 + i % 9,
                    i
                ),
                None,
                None,
            )
            .await,
        );
    }
    let target =
        put(&s, MemoryKind::Semantic, "De oude serverkast in de kelder heeft een defecte ventilator.", None, None)
            .await;
    ids.push(target);
    s.set_tiering(Some(TierConfig { block_size: 64, ..TierConfig::new(d.path().join("hdd"), d.path().join("ssd")) }));
    ids.sort();
    ids.dedup();
    let n = ids.len();
    assert_eq!(s.demote(&ids).unwrap(), n);
    let st = s.tier_stats().unwrap();
    assert_eq!(st.cold as usize, n);
    assert_eq!(st.blocks as usize, n.div_ceil(64));
    assert!(s.get(target).unwrap().content.len() < 200, "stub op de SSD");
    // Zoeken vindt het koude feit, haalt het terug en geeft de volledige inhoud.
    let mut q = Query::new("defecte ventilator serverkast kelder".to_string());
    q.k = 3;
    let r = s.retrieve(&q, None).await.unwrap();
    assert!(
        r.iter().any(|i| i.memory.id == target && i.memory.content.contains("defecte ventilator")),
        "{:?}",
        r.iter().map(|i| &i.memory.content).collect::<Vec<_>>()
    );
    assert_eq!(s.tier_stats().unwrap().cold as usize, n - 1);
    assert!(std::fs::read_dir(d.path().join("ssd")).unwrap().count() >= 1, "blok in de SSD-cache");
    let (us, n) = s.cold_latency(&[0.1; 256]).unwrap();
    assert!(n > 0 && us < 2_000_000);
}

#[tokio::test]
async fn meta_procedures_and_few_shot() {
    let (s, _) = store();
    for i in 0..3 {
        let topic = ["de planning", "het geheugen", "de GUI"][i];
        let mut nm = NewMemory::new(
            MemoryKind::Episodic,
            format!("Gebruiker: hoe staat het met {topic} van Project Vortex?"),
            Provenance::with_origin(SourceKind::User, "chat"),
        );
        nm.force = true;
        s.write(nm).await.unwrap();
    }
    let m = s.meta_report().unwrap();
    assert!(m.total >= 3);
    assert!(m.gaps.iter().any(|g| g.name.to_lowercase().contains("vortex")), "{:?}", m.gaps);
    let p = Procedure {
        name: "excel-rapport".into(),
        description: "Excel-rapport maken".into(),
        body: "Stap 1…".into(),
        success_rate: 0.8,
        uses: 5,
        version: "1".into(),
        origin: "skill".into(),
    };
    assert_eq!(s.sync_procedures(std::slice::from_ref(&p)).await.unwrap(), 1);
    assert_eq!(s.sync_procedures(std::slice::from_ref(&p)).await.unwrap(), 0, "ongewijzigd = niets");
    let p2 = Procedure { version: "2".into(), ..p };
    assert_eq!(s.sync_procedures(&[p2]).await.unwrap(), 1);
    assert_eq!(s.procedures().unwrap().len(), 1, "oude versie verouderd");
    assert!(
        s.remember_example(
            "Hoe maak ik een draaitabel in Excel?",
            "Selecteer de data → Invoegen → Draaitabel.",
            "planning",
            None
        )
        .await
        .unwrap()
    );
    let ex = s.examples_for("draaitabel maken in Excel", 2, None).await.unwrap();
    assert_eq!(ex.len(), 1);
    assert!(ex[0].1.contains("Invoegen"));
}
