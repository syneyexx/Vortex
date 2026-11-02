//! Wat het geheugen van een taalmodel nodig heeft (contextbeschrijving, extractie, samenvatten,
//! query herschrijven). De model-implementatie (router-model op de 1060) zit in `vortex-context`;
//! hier een heuristische terugval die zonder model werkt.

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    #[serde(default = "thing")]
    pub r#type: String,
}

fn thing() -> String {
    "thing".into()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtractedClaim {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    #[serde(default = "pos")]
    pub polarity: i8,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
}

fn pos() -> i8 {
    1
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Extraction {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub claims: Vec<ExtractedClaim>,
}

#[async_trait::async_trait]
pub trait MemLlm: Send + Sync + std::fmt::Debug {
    /// Korte beschrijving van waar dit stuk over gaat in de context van het document
    /// (contextual chunk description). Het origineel blijft apart bewaard.
    async fn describe_context(&self, doc_title: &str, chunk: &str) -> Result<String>;
    async fn extract(&self, text: &str) -> Result<Extraction>;
    async fn summarize(&self, texts: &[String], max_words: usize) -> Result<String>;
    /// Varianten van een zoekvraag (inclusief het origineel).
    async fn rewrite_query(&self, q: &str) -> Result<Vec<String>>;
}

/// Regels zonder model. Grof, maar eerlijk en snel; wordt vervangen zodra het router-model draait.
#[derive(Debug, Clone, Default)]
pub struct HeuristicLlm;

const STOP: &[&str] = &[
    "De", "Het", "Een", "Ik", "Je", "Jij", "Wij", "We", "Zij", "Ze", "Hij", "Dit", "Dat", "Deze", "Die", "En", "Maar",
    "Of", "The", "A", "An", "I", "You", "We", "It", "This", "That", "And", "But", "Or", "In", "Op", "Met", "Voor",
    "Als", "Wat", "Hoe",
];

fn sentences(t: &str) -> Vec<&str> {
    t.split_inclusive(['.', '!', '?', '\n']).map(str::trim).filter(|s| !s.is_empty()).collect()
}

#[async_trait::async_trait]
impl MemLlm for HeuristicLlm {
    async fn describe_context(&self, doc_title: &str, chunk: &str) -> Result<String> {
        let first: String = chunk.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
        Ok(if doc_title.is_empty() { format!("Over: {first}") } else { format!("Uit '{doc_title}': {first}") })
    }

    async fn extract(&self, text: &str) -> Result<Extraction> {
        let mut out = Extraction::default();
        // Entiteiten: reeksen woorden met hoofdletter (niet aan zinsbegin), of met cijfers.
        for s in sentences(text) {
            let words: Vec<&str> = s.split_whitespace().collect();
            let mut cur: Vec<String> = Vec::new();
            for (i, w) in words.iter().enumerate() {
                let clean: String = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_string();
                let cap = clean.chars().next().is_some_and(char::is_uppercase);
                if cap
                    && (i > 0 || !STOP.contains(&clean.as_str()))
                    && !STOP.contains(&clean.as_str())
                    && clean.len() > 1
                {
                    cur.push(clean);
                } else if !cur.is_empty() {
                    let name = cur.join(" ");
                    if !out.entities.iter().any(|e| e.name == name) {
                        out.entities.push(ExtractedEntity { name, r#type: thing() });
                    }
                    cur.clear();
                }
            }
            if !cur.is_empty() {
                let name = cur.join(" ");
                if !out.entities.iter().any(|e| e.name == name) {
                    out.entities.push(ExtractedEntity { name, r#type: thing() });
                }
            }
            // Claims: "X is (niet) Y" / "X heeft Y" / "X kost 12 euro".
            for verb in
                [" is ", " zijn ", " heeft ", " kost ", " weegt ", " woont in ", " werkt bij ", " is not ", " has "]
            {
                if let Some(i) = s.find(verb) {
                    let subject = s[..i].trim().trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
                    let mut object = s[i + verb.len()..].trim().trim_end_matches(['.', '!', '?']).to_string();
                    let mut polarity = 1;
                    if let Some(rest) = object.strip_prefix("niet ").or_else(|| object.strip_prefix("geen ")) {
                        polarity = -1;
                        object = rest.to_string();
                    }
                    if subject.is_empty() || object.is_empty() || subject.split_whitespace().count() > 6 {
                        continue;
                    }
                    let value = object.split_whitespace().next().and_then(|n| n.replace(',', ".").parse::<f64>().ok());
                    let unit = value.and_then(|_| object.split_whitespace().nth(1).map(String::from));
                    out.claims.push(ExtractedClaim {
                        subject: subject.to_lowercase(),
                        predicate: verb.trim().to_string(),
                        object: object.to_lowercase(),
                        polarity,
                        value,
                        unit,
                    });
                    break;
                }
            }
        }
        Ok(out)
    }

    async fn summarize(&self, texts: &[String], max_words: usize) -> Result<String> {
        let mut words = Vec::new();
        for t in texts {
            if let Some(first) = sentences(t).first() {
                words.extend(first.split_whitespace().map(String::from));
            }
            if words.len() >= max_words {
                break;
            }
        }
        words.truncate(max_words);
        Ok(words.join(" "))
    }

    async fn rewrite_query(&self, q: &str) -> Result<Vec<String>> {
        Ok(vec![q.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heuristic_extracts_entities_and_claims() -> Result<()> {
        let x = HeuristicLlm
            .extract("Syneye woont in Amsterdam. De RTX 5060 Ti heeft 16 GB. Python is niet traag.")
            .await?;
        let names: Vec<_> = x.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Syneye") && names.contains(&"Amsterdam"), "{names:?}");
        assert!(x.claims.iter().any(|c| c.subject == "syneye" && c.predicate == "woont in" && c.object == "amsterdam"));
        let neg = x.claims.iter().find(|c| c.subject == "python");
        assert_eq!(neg.map(|c| c.polarity), Some(-1));
        let gb = x.claims.iter().find(|c| c.predicate == "heeft");
        assert_eq!(gb.and_then(|c| c.value), Some(16.0));
        Ok(())
    }
}
