//! Conflict-team (W09.09): twee onderzoekers (elk één kant) → bronbeoordelaar → Judge →
//! Memory Writer (juiste versie blijft, de andere wordt historie, afgeleiden verouderd;
//! tijdgebonden → geldigheidsperiodes; onbeslisbaar → "betwist"). Alles gelogd en terug te
//! draaien ([`MemoryStore::rollback_conflict`]).

use std::sync::Arc;

use serde_json::json;
use vortex_cognition::GenOpts;
use vortex_memory::{Conflict, Memory, MemoryStore, Resolution};
use vortex_models::backend::Message;
use vortex_types::SourceKind;

use crate::json::extract;
use crate::roles::Roles;
use crate::{Llm, Reporter};

fn describe(m: &Memory) -> String {
    format!(
        "\"{}\" — bron: {:?} {}; datum: {}; vertrouwen {:.2}",
        m.content.chars().take(600).collect::<String>(),
        m.provenance.source,
        m.source_uri.clone().or(m.provenance.origin.clone()).unwrap_or_default(),
        m.event_time.clone().unwrap_or(m.created_at.clone()).chars().take(10).collect::<String>(),
        m.confidence
    )
}

fn primary(m: &Memory) -> bool {
    matches!(m.provenance.source, SourceKind::User | SourceKind::File | SourceKind::Tool | SourceKind::Dataset)
}

fn date(m: &Memory) -> String {
    m.event_time.clone().unwrap_or(m.created_at.clone()).chars().take(10).collect()
}

/// Regels die geen model nodig hebben. `None` = het team moet beslissen.
pub fn rule_based(a: &Memory, b: &Memory) -> Option<Resolution> {
    let user = |m: &Memory| m.provenance.source == SourceKind::User;
    match (user(a), user(b)) {
        // Persoonlijke feiten: wat de gebruiker zelf zei wint.
        (true, false) => Some(Resolution::Winner { winner: a.id, reason: "de gebruiker zei dit zelf".into() }),
        (false, true) => Some(Resolution::Winner { winner: b.id, reason: "de gebruiker zei dit zelf".into() }),
        // Beide van de gebruiker: de nieuwste uitspraak geldt vanaf haar datum (oude = historie).
        (true, true) => {
            let (old, new) = if date(a) <= date(b) { (a, b) } else { (b, a) };
            if date(old) == date(new) {
                Some(Resolution::Winner { winner: new.id, reason: "de gebruiker zei dit het laatst".into() })
            } else {
                Some(Resolution::Periods {
                    a_until: if old.id == a.id { date(new) } else { date(old) },
                    b_from: if new.id == b.id { date(new) } else { date(old) },
                    reason: "de gebruiker gaf later een nieuwe waarde (oude blijft als historie)".into(),
                })
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ConflictDeps {
    pub llm: Llm,
    pub roles: Roles,
    pub reporter: Reporter,
}

async fn ask(llm: &Llm, system: &str, temp: Option<f32>, user: String) -> serde_json::Value {
    let out = llm
        .complete(
            vec![Message::system(system.to_string()), Message::user(user)],
            GenOpts { max_tokens: Some(600), temperature: temp, json_schema: None, reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    extract(&out).unwrap_or_default()
}

/// Beslis een conflict en pas het toe. Geeft de beslissing.
pub async fn resolve(d: &ConflictDeps, store: &Arc<MemoryStore>, c: &Conflict) -> Result<Resolution, String> {
    let a = store.get(c.a).map_err(|e| e.to_string())?;
    let b = store.get(c.b).map_err(|e| e.to_string())?;
    let rep = &d.reporter;
    let res = if let Some(r) = rule_based(&a, &b) {
        rep.done("Judge", "conflict beslist op regel (persoonlijk feit)", json!({"conflict": c.id}));
        r
    } else {
        let topic = format!("Tegenstrijdigheid: {}\nA: {}\nB: {}", c.reason, describe(&a), describe(&b));
        rep.started("Onderzoekers", "elk één kant");
        let rr = d.roles.get("conflict-researcher");
        let (pa, pb) = futures::future::join(
            ask(&d.llm, &rr.prompt, rr.temperature, format!("{topic}\n\nJij verdedigt A.")),
            ask(&d.llm, &rr.prompt, rr.temperature, format!("{topic}\n\nJij verdedigt B.")),
        )
        .await;
        rep.done("Onderzoekers", "pleidooien klaar", json!({}));
        rep.started("Bronbeoordelaar", "");
        let sa = d.roles.get("source-assessor");
        let q = ask(&d.llm, &sa.prompt, sa.temperature, format!("{topic}\n\nPleidooi A: {pa}\nPleidooi B: {pb}")).await;
        // Regels bovenop het model: primair > secundair; zelfde bron = één bron.
        let same_source = a.source_uri.is_some() && a.source_uri == b.source_uri;
        let qa = q["a_quality"].as_f64().unwrap_or(0.5) + if primary(&a) { 0.1 } else { 0.0 };
        let qb = q["b_quality"].as_f64().unwrap_or(0.5) + if primary(&b) { 0.1 } else { 0.0 };
        rep.done(
            "Bronbeoordelaar",
            &format!("A {qa:.2} · B {qb:.2}{}", if same_source { " (zelfde bron)" } else { "" }),
            json!({"a": qa, "b": qb}),
        );
        rep.started("Judge", "conflict");
        let jr = d.roles.get("conflict-judge");
        let v = ask(
            &d.llm,
            &jr.prompt,
            jr.temperature,
            format!(
                "{topic}\n\nPleidooi A: {pa}\nPleidooi B: {pb}\nBronkwaliteit: A {qa:.2}, B {qb:.2}. {}",
                q["notes"].as_str().unwrap_or("")
            ),
        )
        .await;
        let reason = v["reason"].as_str().unwrap_or("").to_string();
        let web_only = |m: &Memory| m.provenance.source == SourceKind::Web;
        let r = match v["decision"].as_str().unwrap_or("disputed") {
            // Eén webpagina wint niet automatisch van een even goede bron.
            "a" if web_only(&a) && (qa - qb) < 0.2 => Resolution::Disputed {
                reason: format!("A komt van één webpagina zonder duidelijk betere bron: {reason}"),
            },
            "b" if web_only(&b) && (qb - qa) < 0.2 => Resolution::Disputed {
                reason: format!("B komt van één webpagina zonder duidelijk betere bron: {reason}"),
            },
            "a" => Resolution::Winner { winner: a.id, reason },
            "b" => Resolution::Winner { winner: b.id, reason },
            "periods" => {
                let sw = v["switch_date"]
                    .as_str()
                    .filter(|s| s.len() >= 10)
                    .map(String::from)
                    .unwrap_or_else(|| date(&b).max(date(&a)));
                Resolution::Periods { a_until: sw.clone(), b_from: sw, reason }
            }
            _ => Resolution::Disputed { reason: if reason.is_empty() { "niet te beslissen".into() } else { reason } },
        };
        rep.done("Judge", &format!("{r:?}").chars().take(120).collect::<String>(), json!({"decision": r}));
        r
    };
    rep.started("Memory Writer", "beslissing toepassen");
    store.apply_resolution(&c.id, &res).map_err(|e| e.to_string())?;
    rep.done("Memory Writer", "toegepast (terug te draaien)", json!({"conflict": c.id}));
    Ok(res)
}
