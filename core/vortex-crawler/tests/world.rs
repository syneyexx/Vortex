//! Wereldmonitor tegen een lokale nepserver: RSS, GDELT, USGS, Hacker News, World Bank,
//! opslag met dedup, inhaal-modus, clustering (gemeld vs bevestigd) en bronbias.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use serde_json::json;
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog};
use vortex_crawler::world::cluster::cluster;
use vortex_crawler::world::{SourceSpec, WorldMonitor};

async fn server() -> String {
    let now = chrono::Utc::now();
    let rss_date = now.to_rfc2822();
    let gd = now.format("%Y%m%dT%H%M%SZ").to_string();
    let ms = now.timestamp_millis();
    let ts = now.timestamp();
    let app = Router::new()
        .route(
            "/rss",
            get(move || {
                let d = rss_date.clone();
                async move {
                    format!(
                        "<rss><channel><item><title>Strong earthquake strikes coast of Japan</title><link>https://nos.example/1</link><description>Tsunami warning issued</description><pubDate>{d}</pubDate></item>\
                         <item><title>Dutch cabinet presents budget plan</title><link>https://nos.example/2</link><description>Kabinet</description><pubDate>{d}</pubDate></item></channel></rss>"
                    )
                }
            }),
        )
        .route("/gdelt", get(move || { let g = gd.clone(); async move { axum::Json(json!({"articles": [{"title": "Earthquake strikes Japan coast, tsunami warning", "url": "https://bbc.example/q", "seendate": g, "domain": "bbc.example", "language": "English", "sourcecountry": "United Kingdom"}]})) } }))
        .route("/usgs", get(move || async move { axum::Json(json!({"features": [{"properties": {"title": "M 7.1 - off coast of Honshu, Japan", "mag": 7.1, "place": "Honshu", "time": ms, "url": "https://usgs.example/1"}, "geometry": {"coordinates": [140.0, 38.0, 10.0]}}]})) }))
        .route("/wd", get(|q: axum::extract::Query<std::collections::HashMap<String, String>>| async move {
            let name = q.get("search").cloned().unwrap_or_default();
            axum::Json(if name == "Japan" { json!({"search": [{"id": "Q17", "label": "Japan", "description": "island country in East Asia"}]}) } else { json!({"search": []}) })
        }))
        .route("/hn/topstories.json", get(|| async { axum::Json(json!([1, 2])) }))
        .route("/hn/item/:id", get(move |axum::extract::Path(id): axum::extract::Path<String>| async move {
            let id = id.trim_end_matches(".json").to_string();
            axum::Json(json!({"title": format!("Show HN: project {id}"), "url": format!("https://x.example/{id}"), "time": ts, "score": 10}))
        }))
        .route("/wb/country/NLD/indicator/NY.GDP.MKTP.KD.ZG", get(|| async { axum::Json(json!([{}, [{"indicator": {"value": "GDP growth"}, "country": {"value": "Netherlands"}, "date": "2025", "value": 1.2}, {"indicator": {"value": "GDP growth"}, "country": {"value": "Netherlands"}, "date": "2024", "value": null}]])) }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

fn spec(name: &str, kind: &str, url: String, country: &str) -> SourceSpec {
    SourceSpec {
        name: name.into(),
        kind: kind.into(),
        url,
        interval_min: 30,
        country: Some(country.into()),
        orientation: None,
        enabled: true,
        params: Default::default(),
    }
}

#[tokio::test]
async fn gate_world_monitor_fetch_cluster_bias() {
    let base = server().await;
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    let m = WorldMonitor::new(log.clone());
    let specs = [
        spec("nos", "rss", format!("{base}/rss"), "NL"),
        spec("gdelt", "gdelt", format!("{base}/gdelt"), "INT"),
        spec("usgs", "usgs", format!("{base}/usgs"), "US"),
        spec("hn", "hackernews", format!("{base}/hn"), "US"),
        spec("wb", "worldbank", format!("{base}/wb"), "INT"),
    ];
    for s in &specs {
        assert!(m.due(s));
        let r = m.fetch(s).await;
        assert!(r.error.is_none(), "{}: {:?}", s.name, r.error);
        assert!(r.items >= 1, "{}", s.name);
        assert!(!m.due(s), "interval loopt");
    }
    // Opnieuw ophalen: niets nieuws (dedup op ID).
    assert_eq!(m.fetch(&specs[0]).await.new, 0);
    assert_eq!(m.fetch(&specs[4]).await.items, 1, "null-waarden overgeslagen");

    let rep = cluster(&log, 48, 0.34);
    assert!(rep.assigned >= 5);
    let evs: Vec<(String, String, i64, String)> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT title, status, n_items, theme FROM world_events ORDER BY n_items DESC")?;
            let v = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap();
    let quake = &evs[0];
    assert!(quake.2 >= 2, "aardbeving-artikelen samen: {evs:?}");
    assert_eq!(quake.1, "confirmed");
    assert_eq!(quake.3, "ramp & natuur");
    let budget = evs.iter().find(|e| e.0.contains("budget")).unwrap();
    assert_eq!(budget.1, "reported", "één bron = gemeld");
    // Wikidata-koppeling van bevestigde events.
    assert_eq!(m.link_entities(&format!("{base}/wd"), "en", 10).await, 1);
    let ents: String = log
        .db()
        .read(|c| Ok(c.query_row("SELECT entities FROM world_events WHERE status = 'confirmed'", [], |r| r.get(0))?))
        .unwrap();
    assert!(ents.contains("Q17"), "{ents}");
    assert_eq!(m.link_entities(&format!("{base}/wd"), "en", 10).await, 0, "niet opnieuw");
    // Idempotent: tweede run koppelt niets meer.
    assert_eq!(cluster(&log, 48, 0.34).assigned, 0);
    let bias = m.source_bias();
    assert!(bias.iter().any(|b| b["source"] == "nos" && b["confirmed_share"].as_f64().unwrap() > 0.0));
}
