//! Procedural memory (W12.01): skills en werkwijzen als herinneringen met succesrate en versie
//! (doorzoekbaar zoals de rest; "hoe doe ik X?" vindt de werkwijze) + memory-based few-shot
//! (W12.10): goed beoordeelde antwoorden als voorbeelden voor vergelijkbare vragen.
// VX:W12.01

use serde::{Deserialize, Serialize};
use vortex_types::{Provenance, SourceKind};

use crate::error::Result;
use crate::retrieve::Query;
use crate::store::MemoryStore;
use crate::types::{Filters, MemoryKind, NewMemory, WriteOutcome};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Procedure {
    pub name: String,
    pub description: String,
    pub body: String,
    /// 0..1 (uit skill-statistiek).
    pub success_rate: f64,
    pub uses: i64,
    pub version: String,
    /// "skill" | "werkwijze"
    pub origin: String,
}

impl MemoryStore {
    /// Procedures bijwerken: nieuwe versie of andere succesrate → nieuwe herinnering (oude
    /// versie wordt verouderd via de bron-hash), anders niets.
    pub async fn sync_procedures(&self, procs: &[Procedure]) -> Result<usize> {
        let mut n = 0;
        for p in procs {
            let uri = format!("procedure:{}", p.name);
            let hash = format!("{}|{:.1}", p.version, p.success_rate);
            if self.source_hash(&uri)?.as_deref() == Some(hash.as_str()) {
                continue;
            }
            let body: String = p.body.chars().take(1500).collect();
            let content = format!(
                "Werkwijze \"{}\": {}\n{}\nSuccesrate: {:.0}% ({}× gebruikt) · versie {}",
                p.name,
                p.description,
                body.trim(),
                p.success_rate * 100.0,
                p.uses,
                p.version
            );
            let mut nm = NewMemory::new(
                MemoryKind::Procedural,
                content,
                Provenance::with_origin(SourceKind::System, uri.clone()),
            );
            nm.title = format!("Werkwijze: {}", p.name);
            nm.source_uri = Some(uri);
            nm.source_hash = Some(hash);
            nm.confidence = Some(p.success_rate.clamp(0.05, 0.98));
            nm.tags = vec!["werkwijze".into(), p.origin.clone()];
            nm.force = true;
            if matches!(self.write(nm).await?, WriteOutcome::Stored { .. } | WriteOutcome::Merged { .. }) {
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn procedures(&self) -> Result<Vec<crate::types::Memory>> {
        let all = self.list(&Filters { kinds: vec![MemoryKind::Procedural], ..Default::default() }, None, 1000, 0)?;
        Ok(all.into_iter().filter(|m| m.tags.iter().any(|t| t == "werkwijze")).collect())
    }

    /// Goed antwoord bewaren als voorbeeld (bv. na duim omhoog).
    pub async fn remember_example(
        &self,
        question: &str,
        answer: &str,
        task: &str,
        workspace: Option<String>,
    ) -> Result<bool> {
        let q: String = question.chars().take(600).collect();
        let a: String = answer.chars().take(1800).collect();
        let uri = format!("voorbeeld:{:016x}", vortex_core::config::fnv1a64(q.as_bytes()));
        let mut nm = NewMemory::new(
            MemoryKind::Procedural,
            format!("Vraag: {q}\nGoed antwoord: {a}"),
            Provenance::with_origin(SourceKind::User, uri.clone()),
        );
        nm.title = format!("Voorbeeld: {}", q.chars().take(60).collect::<String>());
        nm.source_uri = Some(uri.clone());
        nm.source_hash = Some(uri);
        nm.workspace = workspace;
        nm.tags = vec!["voorbeeld".into(), task.into()];
        nm.confidence = Some(0.8);
        nm.force = true;
        Ok(matches!(self.write(nm).await?, WriteOutcome::Stored { .. }))
    }

    /// Voorbeelden voor een vraag (few-shot): (vraag, antwoord) van vergelijkbare goede antwoorden.
    pub async fn examples_for(
        &self,
        question: &str,
        k: usize,
        workspace: Option<String>,
    ) -> Result<Vec<(String, String)>> {
        let mut q = Query::new(question.to_string());
        q.k = k * 3;
        q.filters.kinds = vec![MemoryKind::Procedural];
        q.filters.workspace = workspace;
        q.counter_evidence = false;
        let items = self.retrieve(&q, None).await?;
        Ok(items
            .into_iter()
            .filter(|i| i.memory.tags.iter().any(|t| t == "voorbeeld") && i.score.final_score > 0.2)
            .take(k)
            .filter_map(|i| {
                let c = i.memory.content;
                let (qq, aa) = c.strip_prefix("Vraag: ")?.split_once("\nGoed antwoord: ")?;
                Some((qq.to_string(), aa.to_string()))
            })
            .collect())
    }
}
