//! Fact-checker (W09.04): beweringen splitsen (op het 1060-model als dat er is), bewijs per
//! bewering uit het geheugen (lokaal, altijd) en via web_search als we online zijn; daarna één
//! beoordeling per batch. Offline → scope "lokaal" (label "lokaal geverifieerd").

use std::sync::Arc;

use serde_json::json;
use vortex_cognition::GenOpts;
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub};

use crate::evidence::{ClaimCheck, Label, Source};
use crate::json::{extract, strings};
use crate::roles::Roles;
use crate::{Llm, Reporter};

#[derive(Debug, Clone)]
pub struct FactDeps {
    pub llm: Llm,
    pub aux: Option<Llm>,
    pub memory: Option<crate::MemoryAccess>,
    pub tools: Option<Arc<ToolHub>>,
    pub online: bool,
    pub exec: ExecOptions,
}

pub async fn extract_claims(d: &FactDeps, roles: &Roles, text: &str) -> Vec<String> {
    let r = roles.get("factchecker");
    let llm = d.aux.as_ref().unwrap_or(&d.llm);
    let schema = json!({"type": "object", "required": ["claims"], "properties": {"claims": {"type": "array", "items": {"type": "string"}, "maxItems": 6}}});
    let out = llm
        .complete(
            vec![Message::system(r.prompt.clone()), Message::user(text.chars().take(6000).collect::<String>())],
            GenOpts {
                max_tokens: Some(600),
                temperature: r.temperature,
                json_schema: Some(schema),
                reasoning_effort: None,
            },
        )
        .await
        .unwrap_or_default();
    let mut v = extract(&out).map(|v| strings(&v, "claims")).unwrap_or_default();
    v.truncate(6);
    v
}

async fn local_evidence(d: &FactDeps, claim: &str) -> Vec<Source> {
    let Some((store, rr)) = &d.memory else { return vec![] };
    let mut q = vortex_memory::Query::new(claim.to_string());
    q.k = 3;
    q.counter_evidence = true;
    match tokio::time::timeout(std::time::Duration::from_millis(1500), store.retrieve(&q, rr.as_ref())).await {
        Ok(Ok(items)) => items
            .into_iter()
            .filter(|i| i.score.final_score > 0.15)
            .map(|i| Source {
                kind: "geheugen".into(),
                reference: i
                    .memory
                    .source_uri
                    .clone()
                    .or(i.memory.provenance.origin.clone())
                    .unwrap_or_else(|| i.memory.id.to_string()),
                date: Some(
                    i.memory.event_time.clone().unwrap_or(i.memory.created_at.clone()).chars().take(10).collect(),
                ),
                snippet: i.memory.content.chars().take(400).collect(),
            })
            .collect(),
        _ => vec![],
    }
}

async fn web_evidence(d: &FactDeps, claim: &str, n: usize) -> Vec<Source> {
    let (Some(h), true) = (&d.tools, d.online) else { return vec![] };
    let id = format!(
        "factcheck:{}:{}",
        d.exec.run.map(|r| r.to_string()).unwrap_or_default(),
        vortex_tools::stable_hash(claim.as_bytes())
    );
    let r = h.executor.call(&id, "web_search", json!({"query": claim, "max_results": n}), &d.exec).await;
    let Some(o) = r.output.filter(|_| r.ok) else { return vec![] };
    o.data["results"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| Source {
                    kind: "web".into(),
                    reference: x["url"].as_str().unwrap_or("").to_string(),
                    date: x["date"].as_str().map(String::from),
                    snippet: format!("{} — {}", x["title"].as_str().unwrap_or(""), x["snippet"].as_str().unwrap_or("")),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Controleer beweringen. Beweringen zonder enig bewijs = onbevestigd (zonder modelaanroep).
pub async fn check(d: &FactDeps, claims: &[String]) -> Vec<ClaimCheck> {
    let scope = if d.online && d.tools.is_some() { "lokaal+web" } else { "lokaal" };
    let mut gathered = Vec::new();
    for c in claims {
        let mut src = local_evidence(d, c).await;
        src.extend(web_evidence(d, c, 3).await);
        gathered.push((c.clone(), src));
    }
    let with: Vec<usize> = gathered.iter().enumerate().filter(|(_, (_, s))| !s.is_empty()).map(|(i, _)| i).collect();
    let mut labels: Vec<(Label, String)> = vec![(Label::Unverified, "geen bewijs gevonden".into()); gathered.len()];
    if !with.is_empty() {
        let mut prompt = String::from(
            "Beoordeel per bewering of het bewijs haar staaft, tegenspreekt of niets zegt. Wees streng: alleen 'supported' als het bewijs het echt zegt.\n\n",
        );
        for (k, &i) in with.iter().enumerate() {
            let (c, s) = &gathered[i];
            prompt.push_str(&format!("[{k}] Bewering: {c}\nBewijs:\n"));
            for x in s.iter().take(4) {
                prompt.push_str(&format!("  - ({} {}) {}\n", x.kind, x.date.clone().unwrap_or_default(), x.snippet));
            }
        }
        let schema = json!({"type": "object", "required": ["checks"], "properties": {"checks": {"type": "array", "items": {"type": "object", "required": ["i", "label"], "properties": {"i": {"type": "integer"}, "label": {"type": "string", "enum": ["supported", "contradicted", "unverified"]}, "why": {"type": "string"}}}}}});
        let llm = d.aux.as_ref().unwrap_or(&d.llm);
        let out = llm
            .complete(
                vec![Message::system("Je bent een strenge fact-checker. Antwoord als JSON."), Message::user(prompt)],
                GenOpts {
                    max_tokens: Some(800),
                    temperature: Some(0.0),
                    json_schema: Some(schema),
                    reasoning_effort: None,
                },
            )
            .await
            .unwrap_or_default();
        if let Some(v) = extract(&out)
            && let Some(arr) = v["checks"].as_array()
        {
            for x in arr {
                let Some(k) = x["i"].as_u64().map(|k| k as usize) else { continue };
                let Some(&i) = with.get(k) else { continue };
                let l = match x["label"].as_str() {
                    Some("supported") => Label::Supported,
                    Some("contradicted") => Label::Contradicted,
                    _ => Label::Unverified,
                };
                labels[i] = (l, x["why"].as_str().unwrap_or("").to_string());
            }
        }
    }
    gathered
        .into_iter()
        .zip(labels)
        .map(|((claim, sources), (label, why))| ClaimCheck { claim, label, why, sources, scope: scope.into() })
        .collect()
}

/// Rapporteer kort.
pub fn report(rep: &Reporter, checks: &[ClaimCheck]) {
    let s = checks.iter().filter(|c| c.label == Label::Supported).count();
    let x = checks.iter().filter(|c| c.label == Label::Contradicted).count();
    let scope = checks.first().map(|c| c.scope.clone()).unwrap_or_else(|| "lokaal".into());
    rep.done(
        "Fact-checker",
        &format!(
            "{s}/{} gestaafd, {x} tegengesproken ({})",
            checks.len(),
            if scope == "lokaal" { "lokaal geverifieerd" } else { "lokaal + web" }
        ),
        json!({"checks": checks.len(), "supported": s, "contradicted": x, "scope": scope}),
    );
}
