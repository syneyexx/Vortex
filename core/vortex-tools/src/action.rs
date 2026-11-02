//! Getypeerde actie-representatie (W08.02): actie, doel, invoer, verwachte uitvoer,
//! afhankelijkheden, bijwerkingen; door code gevalideerd vóór uitvoering. Chaining (W08.05):
//! `{"$ref": "a1.data.path"}` of `"${a1.text}"` in de invoer verwijst naar eerdere resultaten.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::conditions::Condition;
use crate::contract::Effect;
use crate::error::ToolError;
use crate::tool::ToolOutput;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Actie-ID = idempotentiesleutel (taak-ID). Zelfde ID opnieuw = geen dubbel effect.
    pub id: String,
    pub tool: String,
    /// Doel (pad, URL, tabel…) — voor effectanalyse en weergave.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub input: serde_json::Value,
    /// Wat we verwachten (tekst, voor de Judge/trace).
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub pre: Vec<Condition>,
    #[serde(default)]
    pub post: Vec<Condition>,
}

impl Action {
    pub fn new(id: impl Into<String>, tool: impl Into<String>, input: serde_json::Value) -> Self {
        let input_v = input;
        let target = guess_target(&input_v);
        Self {
            id: id.into(),
            tool: tool.into(),
            target,
            input: input_v,
            expected: None,
            depends_on: vec![],
            pre: vec![],
            post: vec![],
        }
    }
    pub fn after(mut self, ids: &[&str]) -> Self {
        self.depends_on.extend(ids.iter().map(|s| s.to_string()));
        self
    }
    pub fn pre(mut self, c: Condition) -> Self {
        self.pre.push(c);
        self
    }
    pub fn post(mut self, c: Condition) -> Self {
        self.post.push(c);
        self
    }

    /// Verwijzingen naar andere acties in de invoer.
    pub fn refs(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_refs(&self.input, &mut out);
        out.sort();
        out.dedup();
        out
    }
}

/// Doel raden uit gangbare veldnamen.
pub fn guess_target(input: &serde_json::Value) -> Option<String> {
    ["path", "file", "url", "db", "target", "dir"]
        .iter()
        .find_map(|k| input.get(*k).and_then(|v| v.as_str()).map(String::from))
}

fn collect_refs(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(r) = m.get("$ref").and_then(|r| r.as_str()) {
                out.push(r.split('.').next().unwrap_or("").to_string());
            }
            m.values().for_each(|x| collect_refs(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect_refs(x, out)),
        serde_json::Value::String(s) => {
            let mut rest = s.as_str();
            while let Some(i) = rest.find("${") {
                let tail = &rest[i + 2..];
                if let Some(j) = tail.find('}') {
                    out.push(tail[..j].split('.').next().unwrap_or("").to_string());
                    rest = &tail[j + 1..];
                } else {
                    break;
                }
            }
        }
        _ => {}
    }
}

/// Resultaat van één actie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_id: String,
    pub tool: String,
    pub ok: bool,
    #[serde(default)]
    pub output: Option<ToolOutput>,
    #[serde(default)]
    pub error: Option<ToolError>,
    pub effect: Effect,
    pub attempts: u32,
    /// Uit de toolcache.
    pub cached: bool,
    /// Eerder al uitgevoerd (idempotentie-ledger): niets opnieuw gedaan.
    pub replayed: bool,
    pub duration_ms: u64,
}

impl ActionResult {
    /// Tekst voor het model.
    pub fn for_model(&self) -> String {
        match (&self.output, &self.error) {
            (Some(o), _) if self.ok => o.text.clone(),
            (_, Some(e)) => e.for_model(),
            _ => "geen uitvoer".into(),
        }
    }
}

/// Lees `a1.data.x.y` / `a1.text` uit eerdere resultaten.
fn lookup(path: &str, prior: &HashMap<String, ActionResult>) -> Option<serde_json::Value> {
    let mut parts = path.split('.');
    let id = parts.next()?;
    let r = prior.get(id)?;
    let out = r.output.as_ref()?;
    let rest: Vec<&str> = parts.collect();
    match rest.first() {
        None | Some(&"text") if rest.len() <= 1 => Some(serde_json::Value::String(out.text.clone())),
        Some(&"data") => {
            let ptr = format!("/{}", rest[1..].join("/"));
            if rest.len() == 1 { Some(out.data.clone()) } else { out.data.pointer(&ptr).cloned() }
        }
        _ => None,
    }
}

/// Vervang verwijzingen door waarden. Onbekende verwijzing = fout (afhankelijkheid faalde).
pub fn resolve_refs(v: &serde_json::Value, prior: &HashMap<String, ActionResult>) -> Result<serde_json::Value, String> {
    Ok(match v {
        serde_json::Value::Object(m) => {
            if let (Some(r), 1) = (m.get("$ref").and_then(|r| r.as_str()), m.len()) {
                return lookup(r, prior).ok_or_else(|| format!("verwijzing {r} niet beschikbaar"));
            }
            let mut o = serde_json::Map::new();
            for (k, x) in m {
                o.insert(k.clone(), resolve_refs(x, prior)?);
            }
            serde_json::Value::Object(o)
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(|x| resolve_refs(x, prior)).collect::<Result<_, _>>()?)
        }
        serde_json::Value::String(s) if s.contains("${") => {
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(i) = rest.find("${") {
                out.push_str(&rest[..i]);
                let tail = &rest[i + 2..];
                let Some(j) = tail.find('}') else {
                    out.push_str(&rest[i..]);
                    rest = "";
                    break;
                };
                let val =
                    lookup(&tail[..j], prior).ok_or_else(|| format!("verwijzing {} niet beschikbaar", &tail[..j]))?;
                match val {
                    serde_json::Value::String(t) => out.push_str(&t),
                    other => out.push_str(&other.to_string()),
                }
                rest = &tail[j + 1..];
            }
            out.push_str(rest);
            serde_json::Value::String(out)
        }
        other => other.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refs_and_resolution() {
        let a =
            Action::new("a2", "write_file", json!({"path": {"$ref": "a1.data.path"}, "content": "regel: ${a1.text}"}));
        assert_eq!(a.refs(), vec!["a1".to_string()]);
        let mut prior = HashMap::new();
        prior.insert(
            "a1".into(),
            ActionResult {
                action_id: "a1".into(),
                tool: "x".into(),
                ok: true,
                output: Some(ToolOutput { data: json!({"path": "out.txt"}), text: "hallo".into(), preview: None }),
                error: None,
                effect: Effect::Read,
                attempts: 1,
                cached: false,
                replayed: false,
                duration_ms: 0,
            },
        );
        let v = resolve_refs(&a.input, &prior).unwrap();
        assert_eq!(v, json!({"path": "out.txt", "content": "regel: hallo"}));
        assert!(resolve_refs(&json!({"$ref": "zz.text"}), &prior).is_err());
    }
}
