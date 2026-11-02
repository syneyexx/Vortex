//! W05-gate: recall, vergeten overal, backup/restore — plus dedup, conflicten, verouderen,
//! werkruimtes, export/import, versies, decay, samenvatting.

use std::sync::Arc;

use vortex_core::{Db, EventLog, db::CORE_MIGRATIONS};
use vortex_memory::*;
use vortex_types::{EventKind, Provenance, SourceKind};

type R = std::result::Result<(), MemError>;

fn store_at(path: Option<&std::path::Path>) -> Result<MemoryStore> {
    let db = match path {
        Some(p) => Db::open(p, CORE_MIGRATIONS)?,
        None => Db::open_in_memory(CORE_MIGRATIONS)?,
    };
    MemoryStore::open(
        EventLog::new(Arc::new(db)),
        Arc::new(HashEmbedder::new(256)),
        Arc::new(HeuristicLlm),
        MemConfig { dims: 256, ..Default::default() },
    )
}

fn user(t: &str) -> NewMemory {
    let mut m = NewMemory::new(MemoryKind::Semantic, t, Provenance::new(SourceKind::User));
    m.force = true;
    m
}

const FACTS: &[&str] = &[
    "De RTX 5060 Ti heeft 16 GB VRAM en draait het hoofdmodel.",
    "De GTX 1060 draait embeddings, reranker en het router-model.",
    "Vortex gebruikt SQLite als bron van waarheid voor het geheugen.",
    "De Ryzen 9 5950X heeft zestien cores verdeeld over twee CCD's.",
    "Trading wordt pas gebouwd bij het commando TRADING NU MAKEN.",
    "Het debatteam bestaat uit Planner, Solvers, Critic, Judge en Synthesizer.",
    "Ebooks in EPUB en PDF worden met exacte bronverwijzing doorzoekbaar.",
    "De Python-worker doet ingest, parsing en training via gRPC.",
    "Syneye woont in Nederland en schrijft Nederlands.",
    "De GUI is gebouwd met Tauri, React en een WebGL-vortex.",
];

const QUESTIONS: &[(&str, usize)] = &[
    ("Hoeveel VRAM heeft de 5060 Ti?", 0),
    ("Welke kaart doet de embeddings en de reranker?", 1),
    ("Wat is de bron van waarheid voor geheugen?", 2),
    ("Hoeveel cores heeft de processor?", 3),
    ("Wanneer bouwen we trading?", 4),
    ("Uit welke rollen bestaat het debatteam?", 5),
    ("Kan ik ebooks doorzoeken met bronverwijzing?", 6),
    ("Wat doet de Python-worker?", 7),
    ("Waar woont Syneye?", 8),
    ("Waarmee is de GUI gebouwd?", 9),
];

#[tokio::test]
async fn gate_recall_at_k() -> R {
    let s = store_at(None)?;
    let mut ids = Vec::new();
    for f in FACTS {
        match s.write(user(f)).await? {
            WriteOutcome::Stored { ids: i, .. } => ids.push(i[0]),
            o => panic!("{o:?}"),
        }
    }
    let (mut at1, mut at3) = (0, 0);
    for (q, want) in QUESTIONS {
        let mut qq = Query::new(*q);
        qq.k = 3;
        qq.counter_evidence = false;
        let r = s.retrieve(&qq, None).await?;
        let got: Vec<_> = r.iter().map(|x| x.memory.id).collect();
        if got.first() == Some(&ids[*want]) {
            at1 += 1;
        }
        if got.contains(&ids[*want]) {
            at3 += 1;
        }
    }
    // Met de hash-embedder (geen model) als ondergrens; met Qwen3-Embedding hoger.
    assert!(at3 >= 9, "recall@3 = {at3}/10");
    assert!(at1 >= 7, "recall@1 = {at1}/10");
    Ok(())
}

#[tokio::test]
async fn gate_forget_removes_everywhere() -> R {
    let s = store_at(None)?;
    let a = match s.write(user("Mijn geheime projectnaam is Zwaluwstaart en die woont in Kluis Zeven.")).await? {
        WriteOutcome::Stored { ids, .. } => ids[0],
        o => panic!("{o:?}"),
    };
    // Afgeleide samenvatting.
    let mut sm =
        NewMemory::new(MemoryKind::Summary, "Samenvatting: Zwaluwstaart project.", Provenance::new(SourceKind::Memory));
    sm.derived_from = vec![a];
    sm.force = true;
    let sum = match s.write(sm).await? {
        WriteOutcome::Stored { ids, .. } => ids[0],
        o => panic!("{o:?}"),
    };
    s.update(a, "Mijn geheime projectnaam is Zwaluwstaart, opgeslagen in Kluis Zeven.", "correctie").await?;
    assert!(!s.list(&Filters::default(), Some("Zwaluwstaart"), 10, 0)?.is_empty());
    let gone = s.forget(a)?;
    assert!(gone.contains(&a) && gone.contains(&sum), "samenvatting gaat mee");
    // FTS
    assert!(s.list(&Filters { include_stale: true, ..Default::default() }, Some("Zwaluwstaart"), 10, 0)?.is_empty());
    // Vector + hybride
    let r = s.retrieve(&Query::new("Zwaluwstaart Kluis Zeven"), None).await?;
    assert!(r.is_empty(), "niets meer te vinden: {r:?}");
    // Versies, graph, export
    assert!(s.versions(a)?.is_empty());
    let st = s.stats()?;
    assert_eq!(st["entities"], 0, "wees-entiteiten weg: {st}");
    let mut buf = Vec::new();
    s.export(&mut buf, None)?;
    assert!(!String::from_utf8_lossy(&buf).contains("Zwaluwstaart"));
    // Na herladen van de index ook weg.
    assert_eq!(s.load_index()?, 0);
    assert!(s.forget(a).is_err());
    Ok(())
}

#[tokio::test]
async fn gate_backup_and_restore() -> R {
    let d = tempfile::tempdir()?;
    let dbp = d.path().join("v.db");
    let bak = d.path().join("hdd/backup.db");
    {
        let s = store_at(Some(&dbp))?;
        s.write(user("Backup-test: de sleutel ligt onder de mat.")).await?;
        s.backup(&bak)?;
        s.write(user("Na de backup toegevoegd: dit moet na restore weg zijn.")).await?;
    }
    MemoryStore::restore(&bak, &dbp)?;
    let s = store_at(Some(&dbp))?;
    assert_eq!(s.list(&Filters::default(), Some("sleutel"), 10, 0)?.len(), 1);
    assert!(s.list(&Filters::default(), Some("restore"), 10, 0)?.is_empty());
    assert_eq!(s.load_index()?, 1, "vectorindex komt mee terug");
    Ok(())
}

#[tokio::test]
async fn policy_skips_small_talk_and_dedup_merges() -> R {
    let s = store_at(None)?;
    let mut hi = NewMemory::new(MemoryKind::Episodic, "hoi", Provenance::new(SourceKind::User));
    hi.force = false;
    assert!(matches!(s.write(hi).await?, WriteOutcome::Skipped { .. }));
    let a = s.write(user("De backup draait elke nacht om drie uur naar de HDD.")).await?;
    let b = s.write(user("De backup draait elke nacht om drie uur naar de HDD.")).await?;
    let WriteOutcome::Stored { ids, .. } = a else { panic!() };
    assert_eq!(b, WriteOutcome::Merged { into: ids[0] });
    assert_eq!(s.get(ids[0])?.merged_count, 2);
    Ok(())
}

#[tokio::test]
async fn conflicts_are_detected_and_resolvable_with_counter_evidence() -> R {
    let s = store_at(None)?;
    let a = s.write(user("Syneye woont in Amsterdam.")).await?;
    let b = s.write(user("Syneye woont in Utrecht.")).await?;
    let (WriteOutcome::Stored { ids: ia, .. }, WriteOutcome::Stored { ids: ib, conflicts }) = (a, b) else { panic!() };
    assert_eq!(conflicts, 1);
    let c = s.conflicts(Some("open"))?;
    assert_eq!(c.len(), 1);
    assert!(c[0].reason.contains("woont in"));
    // Tegenbewijs-slot bij ophalen.
    let mut q = Query::new("Waar woont Syneye?");
    q.k = 2;
    let r = s.retrieve(&q, None).await?;
    assert!(
        r.iter().any(|x| x.counter_evidence),
        "tegenbewijs gereserveerd: {:?}",
        r.iter().map(|x| &x.memory.content).collect::<Vec<_>>()
    );
    s.resolve_conflict(&c[0].id, Some(ib[0]), "gebruiker verhuisd")?;
    assert!(s.conflicts(Some("open"))?.is_empty());
    let old = s.get(ia[0])?;
    assert!(old.valid_to.is_some() && old.confidence < 0.7, "oude versie naar historie");
    Ok(())
}

#[tokio::test]
async fn changed_source_makes_derived_stale() -> R {
    let s = store_at(None)?;
    let mut m = user("Hoofdstuk 1 van het boek gaat over geheugen.");
    m.source_uri = Some("file:boek.epub".into());
    m.source_hash = Some("v1".into());
    let WriteOutcome::Stored { ids, .. } = s.write(m).await? else { panic!() };
    s.upsert_source("file:boek.epub", "v1")?;
    assert!(!s.get(ids[0])?.stale);
    s.upsert_source("file:boek.epub", "v2")?;
    assert!(s.get(ids[0])?.stale, "bron gewijzigd → verouderd");
    assert!(
        s.retrieve(&Query::new("hoofdstuk geheugen boek"), None).await?.is_empty(),
        "verouderd standaard niet opgehaald"
    );
    Ok(())
}

#[tokio::test]
async fn workspaces_are_isolated_but_share_general_knowledge() -> R {
    let s = store_at(None)?;
    let mut a = user("Project Alpha gebruikt Postgres als database.");
    a.workspace = Some("alpha".into());
    let mut b = user("Project Beta gebruikt MongoDB als database.");
    b.workspace = Some("beta".into());
    s.write(a).await?;
    s.write(b).await?;
    s.write(user("SQLite is een embedded database.")).await?;
    let mut q = Query::new("welke database gebruikt het project?");
    q.filters.workspace = Some("alpha".into());
    q.counter_evidence = false;
    let r = s.retrieve(&q, None).await?;
    let txt: Vec<&str> = r.iter().map(|x| x.memory.content.as_str()).collect();
    assert!(txt.iter().any(|t| t.contains("Postgres")));
    assert!(!txt.iter().any(|t| t.contains("MongoDB")), "andere werkruimte nooit zichtbaar");
    assert!(txt.iter().any(|t| t.contains("SQLite")), "algemene kennis gedeeld");
    assert_eq!(s.workspaces()?, vec!["alpha".to_string(), "beta".to_string()]);
    Ok(())
}

#[tokio::test]
async fn export_import_roundtrip_and_events() -> R {
    let s = store_at(None)?;
    for f in &FACTS[..4] {
        s.write(user(f)).await?;
    }
    let mut buf = Vec::new();
    assert_eq!(s.export(&mut buf, None)?, 4);
    let t = store_at(None)?;
    assert_eq!(t.import(&mut std::io::Cursor::new(buf)).await?, 4);
    assert_eq!(t.list(&Filters::default(), None, 100, 0)?.len(), 4);
    let ev = s.log.read_from(0, 1000)?;
    assert!(ev.iter().any(|e| e.kind == EventKind::MemoryWritten));
    assert!(
        ev.iter().filter(|e| e.kind == EventKind::MemoryWritten).all(|e| e.payload.get("content").is_none()),
        "geen inhoud in events"
    );
    assert!(ev.iter().any(|e| e.kind == EventKind::DecisionMade), "policy-beslissing gelogd");
    Ok(())
}

#[tokio::test]
async fn chunking_long_documents_with_locators_and_summary() -> R {
    let s = store_at(None)?;
    let doc = format!(
        "# Handleiding\n\n## Installatie\n{}\n\n## Gebruik\n{}",
        "Installeer Rust en Node. ".repeat(80),
        "Start met just run. ".repeat(80)
    );
    let mut m = user(&doc);
    m.title = "Handleiding".into();
    m.source_uri = Some("file:handleiding.md".into());
    let WriteOutcome::Stored { ids, .. } = s.write(m).await? else { panic!() };
    assert!(ids.len() >= 2);
    let first = s.get(ids[0])?;
    assert!(first.locator.as_deref().unwrap_or("").contains("Installatie"));
    assert!(first.context.contains("Handleiding"), "contextual chunk description");
    assert!(!first.summary.is_empty(), "lange chunk krijgt samenvatting");
    Ok(())
}

#[tokio::test]
async fn decay_prunes_unimportant_short_term_and_summary_is_derived() -> R {
    let s = store_at(None)?;
    let mut st =
        NewMemory::new(MemoryKind::ShortTerm, "tijdelijke notitie zonder belang", Provenance::new(SourceKind::User));
    st.force = true;
    st.importance = Some(0.05);
    s.write(st).await?;
    let rep = s.decay(0.0, 0.5)?;
    assert_eq!(rep.pruned.len(), 1);
    let sid = vortex_types::SessionId::new();
    for t in ["Ik wil dat Vortex Nederlands praat.", "Ik werk het liefst 's avonds na 19:00."] {
        let mut m = NewMemory::new(MemoryKind::User, t, Provenance::new(SourceKind::User));
        m.session_id = Some(sid);
        m.force = true;
        s.write(m).await?;
    }
    let sum = s.summarize_session(&sid.to_string(), None).await?;
    assert!(sum.is_some());
    Ok(())
}

#[derive(Debug)]
struct Down;
#[async_trait::async_trait]
impl Embedder for Down {
    fn model(&self) -> &str {
        "hash-bow/v1"
    }
    fn dims(&self) -> usize {
        256
    }
    async fn embed(&self, _t: &[String]) -> vortex_memory::Result<Vec<Vec<f32>>> {
        Err(MemError::Embed("1060-server niet bereikbaar".into()))
    }
}

#[tokio::test]
async fn embedder_down_defers_writes_and_retrieval_degrades() -> R {
    let d = tempfile::tempdir()?;
    let p = d.path().join("v.db");
    {
        let s = MemoryStore::open(
            EventLog::new(Arc::new(Db::open(&p, CORE_MIGRATIONS)?)),
            Arc::new(Down),
            Arc::new(HeuristicLlm),
            MemConfig { dims: 256, ..Default::default() },
        )?;
        assert!(s.write_or_defer(user("Uitgestelde herinnering over de GTX 1060.")).await?.is_none());
        assert_eq!(s.retry_pending(10).await?, (0, 1));
        assert!(s.retrieve(&Query::new("GTX 1060"), None).await?.is_empty(), "geen fout, gewoon niets");
    }
    let s = store_at(Some(&p))?;
    assert_eq!(s.retry_pending(10).await?, (1, 0), "model weer terug → wachtrij verwerkt");
    let r = s.retrieve(&Query::new("GTX 1060"), None).await?;
    assert_eq!(r.len(), 1);
    Ok(())
}

#[tokio::test]
async fn tool_results_go_through_policy() -> R {
    let s = store_at(None)?;
    let o = s
        .remember_tool_result(
            "web_search",
            &serde_json::json!({"q": "llama.cpp NVFP4"}),
            "llama.cpp ondersteunt NVFP4 vanaf build b8967 op Blackwell.",
            None,
            None,
        )
        .await?;
    assert!(matches!(o, Some(WriteOutcome::Stored { .. })));
    let m = s.list(&Filters::default(), Some("NVFP4"), 5, 0)?;
    assert_eq!(m[0].provenance.source, SourceKind::Tool);
    assert!(m[0].tags.contains(&"tool".to_string()));
    Ok(())
}
