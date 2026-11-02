//! W14: wereldmodel — KG-integratie, hypotheses, voorspellingen + automatisch toetsen,
//! kalibratie, samenvattingen, spelers (alleen publiek).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use serde_json::json;
use vortex_agents::world::{self, WorldDeps};
use vortex_cognition::{CogLlm, GenOpts};
use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog};
use vortex_crawler::world::{Item, WorldMonitor, cluster::cluster};
use vortex_memory::{HashEmbedder, HeuristicLlm, MemConfig, MemoryStore};
use vortex_models::backend::Message;

#[derive(Debug)]
struct Fake;

#[async_trait::async_trait]
impl CogLlm for Fake {
    async fn complete(&self, m: Vec<Message>, _o: GenOpts) -> Result<String, String> {
        let sys = &m[0].content;
        let user = m.last().map(|x| x.content.clone()).unwrap_or_default();
        Ok(if sys.contains("wereldmodel-analist") {
            json!({"hypotheses": [{"cause": "zware aardbeving", "effect": "tsunami-waarschuwing", "confidence": 0.7, "events": [0]}], "known": []}).to_string()
        } else if sys.contains("toetsbare voorspellingen") {
            json!({"predictions": [{"claim": "Japan meldt naschokken boven M6", "probability": 0.7, "criteria": "USGS meldt M6+ bij Honshu", "resolve_by": "2099-01-01"}]}).to_string()
        } else if sys.contains("Toets deze voorspelling") {
            json!({"verdict": if user.contains("aftershock") { "true" } else { "false" }, "why": "USGS"}).to_string()
        } else if sys.contains("Vat de belangrijkste") {
            json!({"summary": "Zware aardbeving bij Japan (bevestigd).", "top": ["aardbeving"]}).to_string()
        } else if sys.contains("Extraheer uit de bronnen") {
            json!({"records": [{"type": "decision", "text": "investeerde in chips", "context": "tekort", "outcome": "winst", "date": "2025-01-01", "source": 0}, {"type": "failure", "text": "mislukte overname", "source": 0}]}).to_string()
        } else if sys.contains("Voorspel wat deze speler") {
            json!({"claim": "Acme investeert opnieuw in chips", "probability": 0.6, "criteria": "persbericht Acme"})
                .to_string()
        } else {
            "{}".into()
        })
    }
}

fn it(id: &str, src: &str, title: &str, domain: &str, ago_h: i64) -> Item {
    Item {
        id: id.into(),
        source: src.into(),
        kind: "article".into(),
        title: title.into(),
        summary: String::new(),
        url: Some(format!("https://{domain}/{id}")),
        published: Some((chrono::Utc::now() - chrono::Duration::hours(ago_h)).to_rfc3339()),
        lang: None,
        country: Some("JP".into()),
        extra: json!({"domain": domain}),
    }
}

#[tokio::test]
async fn gate_world_model_predicts_and_resolves() {
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    let mem = Arc::new(
        MemoryStore::open(log.clone(), Arc::new(HashEmbedder::new(256)), Arc::new(HeuristicLlm), MemConfig::default())
            .unwrap(),
    );
    let m = WorldMonitor::new(log.clone());
    m.store(&[
        it("wi_1", "nos", "Strong earthquake strikes coast of Japan", "nos.nl", 5),
        it("wi_2", "gdelt", "Earthquake strikes Japan coast, tsunami warning", "bbc.co.uk", 4),
        it("wi_3", "nos", "Acme Corp announces chip investment", "nos.nl", 3),
    ]);
    cluster(&log, 48, 0.34);
    let d = WorldDeps { log: log.clone(), llm: Arc::new(Fake), memory: Some(mem.clone()) };

    // KG: bevestigd event → herinnering (World), events krijgen memory_id.
    assert_eq!(world::integrate(&d, 10).await, 1);
    assert_eq!(world::integrate(&d, 10).await, 0, "idempotent");
    let e = world::events(&log, "2000", Some("confirmed"), 10);
    assert!(e[0].memory_id.is_some());
    assert!(world::event_detail(&log, &e[0].id).unwrap()["items"].as_array().unwrap().len() == 2);

    // Hypotheses: tweede keer gezien → steun 2.
    world::hypothesize(&d, 48).await;
    let h = world::hypothesize(&d, 48).await;
    assert_eq!(h[0].support, 2);

    // Voorspellingen: model + handmatig met verlopen deadline → automatisch toetsen.
    assert_eq!(world::predict(&d, 30, 3).await.len(), 1);
    let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let mut p = world::add_prediction(
        &log,
        "Japan meldt een aftershock boven M6",
        0.8,
        "USGS aftershock M6+ Japan",
        &past,
        None,
        "test",
    )
    .unwrap();
    // Bewijs moet ná het aanmaken binnenkomen.
    let _ = log.db().write(|tx| {
        tx.execute("UPDATE predictions SET created_at = '2000-01-01T00:00:00+00:00' WHERE id = ?1", [&p.id])?;
        Ok(())
    });
    m.store(&[it("wi_4", "usgs", "M 6.4 aftershock near Honshu Japan", "usgs.gov", 0)]);
    let r = world::resolve_due(&d).await;
    assert_eq!(r.len(), 1, "alleen de verlopen voorspelling");
    p = r[0].clone();
    assert_eq!(p.status, "true");
    assert!(world::add_prediction(&log, "x", 0.5, "", "2030-01-01", None, "t").is_err(), "criteria verplicht");
    let cal = world::calibration(&log);
    assert_eq!(cal["overall"]["n"], 1);
    assert!((cal["overall"]["brier"].as_f64().unwrap() - 0.04).abs() < 1e-6);

    // Samenvattingen per thema.
    let s = world::summarize(&d, "day", "theme").await;
    assert!(s.iter().any(|x| x["key"] == "ramp & natuur" && x["summary"].as_str().unwrap().contains("aardbeving")));

    // Spelers: alleen publiek.
    assert!(world::add_player(&log, "Jan Jansen", "tech", "person", "").is_err());
    let pl = world::add_player(&log, "Acme Corp", "halfgeleiders", "organization", "").unwrap();
    let recs = world::study_player(&d, &pl.id).await.unwrap();
    assert_eq!(recs.len(), 2);
    assert!(recs.iter().any(|r| r["type"] == "failure"), "mislukkingen tegen survivorship bias");
    assert!(!world::player(&log, &pl.id).unwrap().sources.is_empty());
    let pp = world::predict_player(&d, &pl.id, "nieuw chiptekort", 60).await.unwrap();
    assert_eq!(pp.actor.as_deref(), Some(pl.id.as_str()));
}
