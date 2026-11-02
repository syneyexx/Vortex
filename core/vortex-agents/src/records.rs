//! Extractor-schema's (W10.09): getypeerde records + validatie. Elk record heeft één ID,
//! herkomst (bron + passage) en status; de Archivist schrijft ze direct naar geheugen + tabel.

use serde_json::{Value, json};

pub const KINDS: &[&str] = &["claim", "number", "entity", "relation", "event", "table", "quote", "source"];

/// JSON-schema per recordsoort.
pub fn schema(kind: &str) -> Value {
    let s = |props: Value, req: &[&str]| json!({"type": "object", "required": req, "properties": props});
    match kind {
        "claim" => s(
            json!({"statement": {"type": "string", "minLength": 5}, "polarity": {"type": "string", "enum": ["positive", "negative"]}, "date": {"type": "string"}, "confidence": {"type": "number", "minimum": 0, "maximum": 1}}),
            &["statement"],
        ),
        "number" => s(
            json!({"value": {"type": "number"}, "unit": {"type": "string"}, "what": {"type": "string", "minLength": 2}, "when": {"type": "string"}, "where": {"type": "string"}, "uncertainty": {"type": "string"}}),
            &["value", "what"],
        ),
        "entity" => s(
            json!({"name": {"type": "string", "minLength": 1}, "type": {"type": "string"}, "aliases": {"type": "array", "items": {"type": "string"}}, "wikidata": {"type": "string"}}),
            &["name", "type"],
        ),
        "relation" => s(
            json!({"subject": {"type": "string"}, "predicate": {"type": "string"}, "object": {"type": "string"}, "valid_from": {"type": "string"}, "valid_to": {"type": "string"}}),
            &["subject", "predicate", "object"],
        ),
        "event" => s(
            json!({"what": {"type": "string", "minLength": 3}, "who": {"type": "string"}, "where": {"type": "string"}, "when": {"type": "string"}, "status": {"type": "string", "enum": ["reported", "confirmed"]}}),
            &["what"],
        ),
        "table" => s(
            json!({"headers": {"type": "array", "items": {"type": "string"}, "minItems": 1}, "rows": {"type": "array", "items": {"type": "array"}}, "units": {"type": "string"}, "notes": {"type": "string"}}),
            &["headers", "rows"],
        ),
        "quote" => s(
            json!({"text": {"type": "string", "minLength": 3, "maxLength": 400}, "speaker": {"type": "string"}, "date": {"type": "string"}}),
            &["text"],
        ),
        "source" => s(
            json!({"url": {"type": "string"}, "type": {"type": "string"}, "publisher": {"type": "string"}, "date": {"type": "string"}}),
            &["url"],
        ),
        _ => json!({"type": "object"}),
    }
}

/// Valideer + coerce één record (`{"kind": …, …velden}`). Fout = reden.
pub fn validate(rec: &Value) -> Result<(String, Value), String> {
    let kind = rec.get("kind").and_then(|k| k.as_str()).ok_or("geen kind")?.to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(format!("onbekende soort {kind}"));
    }
    let mut data = rec.clone();
    if let Some(o) = data.as_object_mut() {
        o.remove("kind");
        o.remove("source");
        o.remove("passage");
    }
    let sc = schema(&kind);
    let data = vortex_tools::schema::coerce(&sc, data);
    vortex_tools::schema::validate(&sc, &data).map_err(|e| e.join("; "))?;
    Ok((kind, data))
}

/// Korte tekst van een record (voor geheugen en weergave).
pub fn render(kind: &str, d: &Value) -> String {
    let g =
        |k: &str| d.get(k).map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default();
    match kind {
        "claim" => g("statement"),
        "number" => format!(
            "{}: {} {}{}{}",
            g("what"),
            g("value"),
            g("unit"),
            if g("when").is_empty() { String::new() } else { format!(" ({})", g("when")) },
            if g("where").is_empty() { String::new() } else { format!(", {}", g("where")) }
        ),
        "entity" => format!("{} ({})", g("name"), g("type")),
        "relation" => format!("{} {} {}", g("subject"), g("predicate"), g("object")),
        "event" => format!(
            "{}{}{}",
            g("what"),
            if g("when").is_empty() { String::new() } else { format!(" — {}", g("when")) },
            if g("where").is_empty() { String::new() } else { format!(", {}", g("where")) }
        ),
        "quote" => format!(
            "\"{}\"{}",
            g("text"),
            if g("speaker").is_empty() { String::new() } else { format!(" — {}", g("speaker")) }
        ),
        "table" => format!("tabel: {}", g("headers")),
        _ => d.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_coerces() {
        let (k, d) =
            validate(&json!({"kind": "number", "value": "4.5", "unit": "GB", "what": "VRAM-gebruik"})).unwrap();
        assert_eq!(k, "number");
        assert_eq!(d["value"], 4.5);
        assert_eq!(render(&k, &d), "VRAM-gebruik: 4.5 GB");
        assert!(validate(&json!({"kind": "number", "value": "veel"})).is_err());
        assert!(validate(&json!({"kind": "gedicht", "x": 1})).is_err());
        assert!(validate(&json!({"kind": "relation", "subject": "a", "predicate": "b", "object": "c"})).is_ok());
    }
}
