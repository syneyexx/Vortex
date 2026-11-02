//! Toolcall-parsers per model, geversioneerd. Ongeldige uitvoer = fout, **nooit** een actie.
//!
//! * `openai-native/v1` — `tool_calls` uit de OpenAI-compatibele API (llama-server `--jinja`).
//! * `hermes-xml/v1` — `<tool_call>{"name":…,"arguments":…}</tool_call>` in de tekst (fallback
//!   voor modellen zonder native function calling).
// VX:W02.05

use crate::backend::{ToolCall, ToolSpec};
use crate::error::{ModelError, Result};

/// Een parser; naam + versie worden mee gelogd zodat elke toolcall herleidbaar is.
pub trait ToolCallParser: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &'static str;
    /// Haal toolaanroepen uit (tekst, ruwe native calls). Geeft de resterende tekst terug.
    fn parse(&self, text: &str, native: &[RawToolCall], tools: &[ToolSpec]) -> Result<(String, Vec<ToolCall>)>;
}

/// Ruwe native toolaanroep zoals de API hem levert (argumenten als string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

fn invalid(parser: &str, reason: impl Into<String>) -> ModelError {
    ModelError::InvalidToolCall { parser: parser.into(), reason: reason.into() }
}

/// Valideer naam (bestaat de tool?) en argumenten (object, verplichte velden, simpele typen).
pub fn validate(parser: &str, name: &str, args: &serde_json::Value, tools: &[ToolSpec]) -> Result<()> {
    let spec =
        tools.iter().find(|t| t.name == name).ok_or_else(|| invalid(parser, format!("onbekende tool '{name}'")))?;
    let obj = args.as_object().ok_or_else(|| invalid(parser, format!("argumenten van '{name}' zijn geen object")))?;
    let schema = &spec.parameters;
    if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
        for f in req.iter().filter_map(|f| f.as_str()) {
            if !obj.contains_key(f) {
                return Err(invalid(parser, format!("'{name}': verplicht veld '{f}' ontbreekt")));
            }
        }
    }
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (k, v) in obj {
            let Some(p) = props.get(k) else {
                if schema.get("additionalProperties") == Some(&serde_json::Value::Bool(false)) {
                    return Err(invalid(parser, format!("'{name}': onbekend veld '{k}'")));
                }
                continue;
            };
            if let Some(ty) = p.get("type").and_then(|t| t.as_str()) {
                let ok = match ty {
                    "string" => v.is_string(),
                    "integer" => v.is_i64() || v.is_u64(),
                    "number" => v.is_number(),
                    "boolean" => v.is_boolean(),
                    "array" => v.is_array(),
                    "object" => v.is_object(),
                    "null" => v.is_null(),
                    _ => true,
                };
                if !ok {
                    return Err(invalid(parser, format!("'{name}': veld '{k}' moet {ty} zijn")));
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct OpenAiNativeParser;

impl ToolCallParser for OpenAiNativeParser {
    fn id(&self) -> &'static str {
        "openai-native/v1"
    }
    fn parse(&self, text: &str, native: &[RawToolCall], tools: &[ToolSpec]) -> Result<(String, Vec<ToolCall>)> {
        let mut out = Vec::with_capacity(native.len());
        for (i, raw) in native.iter().enumerate() {
            let a = if raw.arguments.trim().is_empty() { "{}" } else { raw.arguments.as_str() };
            let args: serde_json::Value = serde_json::from_str(a)
                .map_err(|e| invalid(self.id(), format!("argumenten van '{}' geen JSON: {e}", raw.name)))?;
            validate(self.id(), &raw.name, &args, tools)?;
            let id = if raw.id.is_empty() { format!("call_{i}") } else { raw.id.clone() };
            out.push(ToolCall { id, name: raw.name.clone(), arguments: args });
        }
        Ok((text.to_string(), out))
    }
}

#[derive(Debug, Default)]
pub struct HermesXmlParser;

impl ToolCallParser for HermesXmlParser {
    fn id(&self) -> &'static str {
        "hermes-xml/v1"
    }
    fn parse(&self, text: &str, native: &[RawToolCall], tools: &[ToolSpec]) -> Result<(String, Vec<ToolCall>)> {
        if !native.is_empty() {
            return OpenAiNativeParser.parse(text, native, tools);
        }
        const OPEN: &str = "<tool_call>";
        const CLOSE: &str = "</tool_call>";
        let mut rest = String::new();
        let mut calls = Vec::new();
        let mut s = text;
        while let Some(i) = s.find(OPEN) {
            rest.push_str(&s[..i]);
            let after = &s[i + OPEN.len()..];
            let j = after.find(CLOSE).ok_or_else(|| invalid(self.id(), "<tool_call> zonder afsluiting"))?;
            let body: serde_json::Value =
                serde_json::from_str(after[..j].trim()).map_err(|e| invalid(self.id(), format!("geen JSON: {e}")))?;
            let name = body.get("name").and_then(|n| n.as_str()).ok_or_else(|| invalid(self.id(), "naam ontbreekt"))?;
            let args = body.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            validate(self.id(), name, &args, tools)?;
            calls.push(ToolCall { id: format!("call_{}", calls.len()), name: name.into(), arguments: args });
            s = &after[j + CLOSE.len()..];
        }
        rest.push_str(s);
        Ok((rest.trim().to_string(), calls))
    }
}

/// Parser op naam (uit de registry).
pub fn parser_by_id(id: &str) -> Result<Box<dyn ToolCallParser>> {
    match id {
        "openai-native/v1" => Ok(Box::new(OpenAiNativeParser)),
        "hermes-xml/v1" => Ok(Box::new(HermesXmlParser)),
        other => Err(ModelError::Registry(format!("onbekende toolcall-parser '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "read_file".into(),
            description: "lees".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}, "max": {"type": "integer"}},
                "required": ["path"],
                "additionalProperties": false
            }),
        }]
    }

    fn raw(name: &str, args: &str) -> RawToolCall {
        RawToolCall { id: "c1".into(), name: name.into(), arguments: args.into() }
    }

    #[test]
    fn native_valid_call() -> Result<()> {
        let (_, c) = OpenAiNativeParser.parse("", &[raw("read_file", r#"{"path":"a"}"#)], &tools())?;
        assert_eq!(c[0].arguments["path"], "a");
        Ok(())
    }

    #[test]
    fn invalid_calls_are_errors_never_actions() {
        let t = tools();
        for (n, a) in [
            ("bestaat_niet", r#"{"path":"a"}"#),
            ("read_file", r#"{"path":"a""#),
            ("read_file", r#"{}"#),
            ("read_file", r#"{"path": 3}"#),
            ("read_file", r#"{"path":"a","x":1}"#),
            ("read_file", r#"[1]"#),
        ] {
            assert!(
                matches!(OpenAiNativeParser.parse("", &[raw(n, a)], &t), Err(ModelError::InvalidToolCall { .. })),
                "{n} {a}"
            );
        }
    }

    #[test]
    fn hermes_xml_extracts_calls_and_keeps_text() -> Result<()> {
        let txt = "Even kijken.\n<tool_call>{\"name\":\"read_file\",\"arguments\":{\"path\":\"x\"}}</tool_call>";
        let (rest, c) = HermesXmlParser.parse(txt, &[], &tools())?;
        assert_eq!(rest, "Even kijken.");
        assert_eq!(c.len(), 1);
        assert!(HermesXmlParser.parse("<tool_call>{\"name\":\"read_file\"", &[], &tools()).is_err());
        Ok(())
    }

    #[test]
    fn parser_lookup() {
        assert!(parser_by_id("openai-native/v1").is_ok());
        assert!(parser_by_id("x").is_err());
    }
}
