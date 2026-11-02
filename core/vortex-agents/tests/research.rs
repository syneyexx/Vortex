//! W10: research-team end-to-end met nep-SearXNG + twee "domeinen" (127.0.0.1 en localhost),
//! een ingelezen document in het geheugen en een gescript model. Offline: alleen lokaal.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use serde_json::json;
use vortex_agents::research::{ResearchDeps, ResearchMode, corpus_synthesis, run};
use vortex_agents::{Reporter, research};
use vortex_cognition::{CogLlm, GenOpts};
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog, ResourceRegistry, Supervisor};
use vortex_crawler::{CrawlConfig, Crawler};
use vortex_memory::{HashEmbedder, HeuristicLlm, MemConfig, MemoryKind, MemoryStore, NewMemory};
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub, ToolHubConfig};
use vortex_types::{EventKind, Provenance, SourceKind, TraceId};

#[derive(Debug)]
struct Fake;

#[async_trait::async_trait]
impl CogLlm for Fake {
    async fn complete(&self, m: Vec<Message>, _o: GenOpts) -> Result<String, String> {
        let sys = &m[0].content;
        let user = m.last().map(|x| x.content.clone()).unwrap_or_default();
        Ok(if sys.contains("Research Lead") {
            json!({"subquestions": ["Hoeveel VRAM gebruikt QLoRA?", "Waarom werkt QLoRA met weinig geheugen?"], "hypotheses": ["4-bit kwantisatie"], "stop": "beide beantwoord"}).to_string()
        } else if sys.contains("Scout") {
            json!({"queries": ["qlora vram"]}).to_string()
        } else if sys.contains("Extractor") {
            json!({"records": [
                {"kind": "number", "value": "6", "unit": "GB", "what": "VRAM voor QLoRA 7B", "passage": 0},
                {"kind": "claim", "statement": "QLoRA gebruikt 4-bit kwantisatie van de basisgewichten", "passage": 0},
                {"kind": "number", "value": "veel", "what": "ongeldig"}
            ]})
            .to_string()
        } else if sys.contains("Analyst") {
            json!({"answer": "QLoRA past met 4-bit kwantisatie in ongeveer 6 GB VRAM [1][2].", "cited": [1, 2], "confidence": 0.8}).to_string()
        } else if sys.contains("Judge van het research-team") {
            json!({"enough": true, "reason": "twee bronnen"}).to_string()
        } else if sys.contains("Writer") {
            format!(
                "# Rapport\n\nKort antwoord: QLoRA past in ~6 GB VRAM [1].\n\n{}",
                user.lines().skip(2).collect::<Vec<_>>().join("\n")
            )
        } else if sys.contains("Wat zegt dit document") {
            "Het document noemt 4-bit kwantisatie.".into()
        } else if sys.contains("Synthetiseer") {
            "Over de hele verzameling: 4-bit kwantisatie is de kern [D1].".into()
        } else {
            "{}".into()
        })
    }
}

fn page(topic: &str) -> String {
    format!(
        "<html><head><title>{topic}</title></head><body><main><h1>{topic}</h1><p>QLoRA gebruikt 4-bit kwantisatie van de basisgewichten en traint kleine adapters. Een 7B-model past daardoor in ongeveer 6 GB VRAM tijdens het trainen.</p><p>Dit maakt finetunen mogelijk op consumentenkaarten met weinig geheugen.</p></main></body></html>"
    )
}

async fn web() -> (String, u16) {
    let app = Router::new()
        .route("/a", get(|| async { axum::response::Html(page("QLoRA uitleg A")) }))
        .route("/b", get(|| async { axum::response::Html(page("QLoRA uitleg B")) }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let p = port;
    let searx = Router::new().route(
        "/search",
        get(move || async move {
            axum::Json(json!({"results": [
                {"title": "QLoRA A", "url": format!("http://127.0.0.1:{p}/a"), "content": "qlora"},
                {"title": "QLoRA B", "url": format!("http://localhost:{p}/b"), "content": "qlora"}
            ]}))
        }),
    );
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let s = format!("http://{}", l2.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l2, searx).await.unwrap() });
    (s, port)
}

struct Fx {
    deps: ResearchDeps,
    store: Arc<MemoryStore>,
    log: EventLog,
    _d: tempfile::TempDir,
}

async fn fx(online: bool) -> Fx {
    let (searx, _) = web().await;
    let d = tempfile::tempdir().unwrap();
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    let store = Arc::new(
        MemoryStore::open(log.clone(), Arc::new(HashEmbedder::new(256)), Arc::new(HeuristicLlm), MemConfig::default())
            .unwrap(),
    );
    // Ingelezen document met exacte locator.
    let mut nm = NewMemory::new(
        MemoryKind::Semantic,
        "QLoRA traint adapters bovenop een 4-bit gekwantiseerd basismodel, zodat het VRAM-gebruik laag blijft.",
        Provenance::with_origin(SourceKind::File, "/boeken/llm.epub"),
    );
    nm.source_uri = Some("/boeken/llm.epub".into());
    nm.locator = Some("hoofdstuk 7 > QLoRA".into());
    nm.title = "LLM-boek › QLoRA".into();
    nm.force = true;
    store.write(nm).await.unwrap();
    let hub = ToolHub::new(
        ToolHubConfig {
            searxng_url: Some(searx),
            skills_dir: Some(d.path().join("s")),
            workdir: Some(d.path().join("w")),
            ..Default::default()
        },
        log.clone(),
        Supervisor::new(log.clone(), ResourceRegistry::new(log.clone())),
        Arc::new(|_| None),
    );
    let crawler = Arc::new(Crawler::new(
        log.clone(),
        CrawlConfig { domain_interval_ms: 10, raw_dir: Some(d.path().join("cold")), ..Default::default() },
    ));
    let run = vortex_types::RunId::new();
    let deps = ResearchDeps {
        main: Arc::new(Fake),
        aux: None,
        memory: Some((store.clone(), None)),
        mem_workspace: None,
        tools: Some(hub),
        crawler: Some(crawler),
        online,
        reporter: Reporter::new(Some(log.clone()), Some(run), TraceId::new()),
        exec: ExecOptions { run: Some(run), ..Default::default() },
        workspace: None,
        log: log.clone(),
        research_id: "res_test".into(),
        out_dir: Some(d.path().join("reports")),
    };
    Fx { deps, store, log, _d: d }
}

#[tokio::test]
async fn standard_research_end_to_end_with_records_and_citations() {
    let f = fx(true).await;
    let o = run(&f.deps, "Hoeveel geheugen heeft QLoRA nodig en waarom?", ResearchMode::Standard).await;
    assert_eq!(o.subquestions.len(), 2);
    assert!(o.subquestions.iter().all(|s| s.status == "answered"), "{:?}", o.subquestions);
    let kinds: Vec<&str> = o.sources.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"web") && kinds.contains(&"document"), "{kinds:?}");
    let doc = o.sources.iter().find(|s| s.kind == "document").unwrap();
    assert_eq!(doc.locator.as_deref(), Some("hoofdstuk 7 > QLoRA"), "exacte bronverwijzing");
    assert!(o.report.contains("[1]") && o.report.contains("## Bronnen"), "{}", o.report);
    assert!(o.report.contains("| VRAM voor QLoRA 7B | 6.0 | GB"), "getallentabel: {}", o.report);
    assert!(std::path::Path::new(o.report_path.as_ref().unwrap()).exists(), "rapport als artefact");
    assert!(o.records >= 2 && o.invalid_records >= 1, "validatie verwerpt ongeldige records");
    // Records in de tabel; getrianguleerd (twee domeinen met dezelfde passage).
    let sup: i64 = f
        .log
        .db()
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM research_records WHERE research_id = 'res_test' AND status = 'supported'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(sup >= 1);
    // Archivist: records direct in het geheugen met herkomst.
    let mems = f.store.list(&vortex_memory::Filters::default(), None, 200, 0).unwrap();
    let dbg: Vec<_> = mems.iter().map(|m| (m.tags.clone(), m.source_uri.clone(), m.title.clone())).collect();
    let mids: Vec<Option<String>> = f
        .log
        .db()
        .read(|c| {
            let mut s = c.prepare("SELECT memory_id FROM research_records")?;
            let v = s.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap();
    assert!(
        mems.iter().any(|m| m.tags.contains(&"research".to_string())
            && m.tags.contains(&"number".to_string())
            && m.locator.is_some()),
        "{dbg:?} {mids:?}"
    );
    assert!(mids.iter().all(|m| m.is_some()), "elk record gekoppeld aan een herinnering: {mids:?}");
    // Events voor de Research-pagina.
    let ev = f.log.read_from(0, 5000).unwrap();
    for k in [
        EventKind::ResearchStarted,
        EventKind::ResearchSubquestion,
        EventKind::ResearchSource,
        EventKind::ResearchRecord,
        EventKind::ResearchReport,
    ] {
        assert!(ev.iter().any(|e| e.kind == k), "{k:?}");
    }
    // Opgeslagen run.
    let st: String = f
        .log
        .db()
        .read(|c| Ok(c.query_row("SELECT status FROM research_runs WHERE id = 'res_test'", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(st, "klaar");
}

#[tokio::test]
async fn offline_research_uses_only_local_knowledge() {
    let f = fx(false).await;
    let o = run(&f.deps, "Waarom werkt QLoRA met weinig geheugen?", ResearchMode::Quick).await;
    assert_eq!(o.scope, "lokaal");
    assert!(o.sources.iter().all(|s| s.kind != "web"));
    assert!(!o.sources.is_empty());
    let ev = f.log.read_from(0, 5000).unwrap();
    assert!(!ev.iter().any(|e| e.kind == EventKind::ToolCallStarted), "geen webcalls offline");
}

#[tokio::test]
async fn corpus_synthesis_over_collection() {
    let f = fx(false).await;
    let llm: vortex_agents::Llm = Arc::new(Fake);
    let s = corpus_synthesis(&llm, None, &f.store, None, "Wat is de rode draad?").await;
    assert!(s.contains("[D1]"), "{s}");
    assert_eq!(research::choose_mode("research", 0.1, 3), ResearchMode::Deep);
    assert_eq!(research::choose_mode("adaptive", 0.2, 5), ResearchMode::Quick);
}
