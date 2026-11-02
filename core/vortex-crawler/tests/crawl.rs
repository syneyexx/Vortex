//! Crawler-tests tegen een lokale site: robots.txt, focused volgorde, dedup, tempo per domein,
//! frontier in SQLite, ruwe HTML op de koude schijf, cache, PDF als document, stoppen op budget.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use axum::Router;
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::get;
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog};
use vortex_crawler::{CrawlConfig, Crawler};
use vortex_types::TraceId;

fn long(topic: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("Zin {i} over {topic} met eigen inhoud nummer {i} en details {}.", i * 7))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn site(hits: Arc<AtomicUsize>) -> String {
    let h = hits.clone();
    let app = Router::new()
        .route("/robots.txt", get(|| async { "User-agent: *\nDisallow: /privé\nDisallow: /private\n" }))
        .route(
            "/",
            get(|| async {
                axum::response::Html(format!(
                    "<html><head><title>Start</title></head><body><main><p>{}</p>\
                     <a href='/qlora'>QLoRA geheugen uitleg</a> <a href='/contact'>Contact</a> \
                     <a href='/private/geheim'>QLoRA geheim</a> <a href='/kopie'>QLoRA kopie</a> <a href='/paper.pdf'>QLoRA paper</a></main></body></html>",
                    long("de startpagina", 30)
                ))
            }),
        )
        .route("/qlora", get(|| async { axum::response::Html(format!("<html><title>QLoRA</title><main><p>{}</p><a href='/qlora/diep'>QLoRA diepte</a></main></html>", long("qlora geheugen", 40))) }))
        .route("/kopie", get(|| async { axum::response::Html(format!("<html><title>Kopie</title><main><p>{}</p></main></html>", long("qlora geheugen", 40))) }))
        .route("/qlora/diep", get(|| async { axum::response::Html(format!("<html><title>Diep</title><main><p>{}</p></main></html>", long("adapters en rang", 40))) }))
        .route("/contact", get(|| async { axum::response::Html("<html><main><p>Bel ons.</p></main></html>") }))
        .route("/private/geheim", get(|| async { "mag niet" }))
        .route("/paper.pdf", get(|| async { ([("content-type", "application/pdf")], b"%PDF-1.4 nep".to_vec()).into_response() }))
        .route("/x/:n", get(|Path(n): Path<u32>| async move { axum::response::Html(format!("<html><main><p>{}</p></main></html>", long(&format!("onderwerp {n}"), 30))) }))
        .layer(axum::middleware::from_fn(move |req: axum::extract::Request, next: axum::middleware::Next| {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                next.run(req).await
            }
        }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

fn log() -> EventLog {
    EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()))
}

#[tokio::test]
async fn focused_polite_crawl_with_dedup_and_cold_storage() {
    let hits = Arc::new(AtomicUsize::new(0));
    let base = site(hits.clone()).await;
    let d = tempfile::tempdir().unwrap();
    let l = log();
    let cfg = CrawlConfig {
        max_pages: 10,
        domain_interval_ms: 120,
        raw_dir: Some(d.path().to_path_buf()),
        ..Default::default()
    };
    let c = Crawler::new(l.clone(), cfg);
    let t = Instant::now();
    let rep =
        c.crawl("taak-1", "QLoRA geheugen", vec![format!("{base}/")], None, TraceId::new(), Default::default()).await;
    let urls: Vec<String> = rep.pages.iter().map(|p| p.final_url.replace(&base, "")).collect();
    assert!(urls.contains(&"/".to_string()) && urls.contains(&"/qlora".to_string()), "{urls:?}");
    assert!(
        urls.iter().position(|u| u == "/qlora") < urls.iter().position(|u| u == "/qlora/diep").or(Some(usize::MAX))
    );
    assert!(!urls.iter().any(|u| u.contains("private")), "robots.txt gerespecteerd");
    assert_eq!(rep.robots_blocked, 1);
    assert!(rep.duplicates >= 1, "kopie herkend als duplicaat");
    assert_eq!(rep.documents.len(), 1, "PDF als document voor ingest");
    assert!(std::path::Path::new(&rep.documents[0]).exists());
    // Ruwe HTML op de koude schijf.
    assert!(rep.pages.iter().all(|p| p.raw_path.as_ref().is_some_and(|r| std::path::Path::new(r).exists())));
    // Tempo: ≥ 5 verzoeken naar één host met 120 ms ertussen.
    assert!(t.elapsed().as_millis() as usize >= 120 * (hits.load(Ordering::SeqCst) - 1).min(5), "per-domein tempo");
    // Frontier in SQLite.
    let n: i64 = l
        .db()
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM crawl_frontier WHERE task_id = 'taak-1'", [], |r| r.get(0))?))
        .unwrap();
    assert!(n >= 6);
    // Tweede crawl: uit de cache, geen nieuwe paginaverzoeken (alleen robots.txt bestaat al in geheugen).
    let before = hits.load(Ordering::SeqCst);
    let rep2 = c
        .crawl("taak-2", "QLoRA geheugen", vec![format!("{base}/qlora")], None, TraceId::new(), Default::default())
        .await;
    assert!(rep2.from_cache >= 1);
    assert!(hits.load(Ordering::SeqCst) - before <= 2, "cache gebruikt");
    // Leren: betrouwbaarheid per domein.
    let dom = rep.pages[0].domain.clone();
    let r0 = c.reliability(&dom);
    c.record_outcome(&dom, true, 3, 0);
    assert!(c.reliability(&dom) > r0);
}

#[tokio::test]
async fn stops_on_page_budget() {
    let base = site(Arc::new(AtomicUsize::new(0))).await;
    let c = Crawler::new(log(), CrawlConfig { max_pages: 2, domain_interval_ms: 10, ..Default::default() });
    let seeds = (0..6).map(|i| format!("{base}/x/{i}")).collect();
    let rep = c.crawl("t", "onderwerp", seeds, None, TraceId::new(), Default::default()).await;
    assert_eq!(rep.pages.len(), 2);
    assert_eq!(rep.stopped, "paginabudget bereikt");
}
