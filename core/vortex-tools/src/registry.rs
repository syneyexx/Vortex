//! Toolregistry (W08.01) + tool discovery (W08.09): het model ziet eerst alleen naam +
//! beschrijving; het schema komt pas bij gebruik (`describe_tool`) of voor de kernset.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::contract::ToolContract;
use crate::tool::Tool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolBrief {
    pub name: String,
    pub description: String,
    pub effect: crate::contract::Effect,
    pub origin: String,
}

#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: RwLock<BTreeMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registreer (vervangt een bestaande tool met dezelfde naam = nieuwe versie).
    pub fn register(&self, t: Arc<dyn Tool>) {
        let name = t.contract().name.clone();
        self.tools.write().insert(name, t);
    }

    /// Alles van één herkomst weghalen (bv. MCP-server herladen).
    pub fn unregister_origin(&self, origin: &str) -> usize {
        let mut g = self.tools.write();
        let before = g.len();
        g.retain(|_, t| t.contract().origin != origin);
        before - g.len()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().get(name).cloned()
    }

    pub fn contract(&self, name: &str) -> Option<ToolContract> {
        self.get(name).map(|t| t.contract().clone())
    }

    pub fn contracts(&self) -> Vec<ToolContract> {
        self.tools.read().values().map(|t| t.contract().clone()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.read().keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.tools.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Discovery: naam + beschrijving (geen schema's).
    pub fn briefs(&self) -> Vec<ToolBrief> {
        self.tools
            .read()
            .values()
            .map(|t| {
                let c = t.contract();
                ToolBrief {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    effect: c.effect,
                    origin: c.origin.clone(),
                }
            })
            .collect()
    }

    /// Tekstblok voor de prompt: één regel per tool.
    pub fn briefs_text(&self, except: &[String]) -> String {
        self.briefs()
            .into_iter()
            .filter(|b| !except.contains(&b.name))
            .map(|b| format!("- {}: {}", b.name, b.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Dichtstbijzijnde namen (voor "bestaat niet"-hints).
    pub fn closest(&self, name: &str, n: usize) -> Vec<String> {
        let mut v: Vec<(usize, String)> = self.names().into_iter().map(|k| (edit_distance(name, &k), k)).collect();
        v.sort();
        v.into_iter().take(n).map(|(_, k)| k).collect()
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let c = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + c);
        }
        prev = cur;
    }
    prev[b.len()]
}
