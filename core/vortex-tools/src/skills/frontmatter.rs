//! YAML-frontmatter (subset): `key: waarde`, lijsten (`[a, b]` of `- a`-regels), booleans,
//! strings met/zonder quotes, meerregelige `>`/`|`-blokken.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    List(Vec<String>),
    Bool(bool),
}

impl Val {
    pub fn as_str(&self) -> String {
        match self {
            Val::Str(s) => s.clone(),
            Val::List(l) => l.join(", "),
            Val::Bool(b) => b.to_string(),
        }
    }
    pub fn as_list(&self) -> Vec<String> {
        match self {
            Val::List(l) => l.clone(),
            Val::Str(s) if s.trim().is_empty() => vec![],
            Val::Str(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
            Val::Bool(_) => vec![],
        }
    }
    pub fn as_bool(&self) -> bool {
        match self {
            Val::Bool(b) => *b,
            Val::Str(s) => matches!(s.trim().to_lowercase().as_str(), "true" | "yes" | "ja"),
            Val::List(_) => false,
        }
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))) {
        t[1..t.len() - 1].replace("\\\"", "\"").to_string()
    } else {
        t.to_string()
    }
}

/// Splits `---\nfrontmatter\n---\nbody`. Zonder frontmatter: lege map + hele tekst.
pub fn split(text: &str) -> (BTreeMap<String, Val>, String) {
    let t = text.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---") else { return (BTreeMap::new(), t.to_string()) };
    let rest = rest.trim_start_matches(['\r', ' ']).strip_prefix('\n').unwrap_or(rest);
    let Some(end) = rest.find("\n---") else { return (BTreeMap::new(), t.to_string()) };
    let fm = &rest[..end];
    let after = &rest[end + 4..];
    let body = after.split_once('\n').map_or("", |(_, b)| b).to_string();
    (parse(fm), body)
}

pub fn parse(fm: &str) -> BTreeMap<String, Val> {
    let mut out = BTreeMap::new();
    let lines: Vec<&str> = fm.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.starts_with(' ')
            || line.starts_with('-')
        {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let key = k.trim().to_string();
        let v = v.trim();
        let val = if v == ">" || v == "|" || v == ">-" || v == "|-" {
            let mut buf = Vec::new();
            while i < lines.len() && (lines[i].starts_with(' ') || lines[i].trim().is_empty()) {
                buf.push(lines[i].trim());
                i += 1;
            }
            Val::Str(if v.starts_with('>') {
                buf.join(" ").trim().to_string()
            } else {
                buf.join("\n").trim().to_string()
            })
        } else if v.is_empty() {
            let mut items = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with("- ") {
                items.push(unquote(lines[i].trim_start().trim_start_matches("- ")));
                i += 1;
            }
            if items.is_empty() { Val::Str(String::new()) } else { Val::List(items) }
        } else if v.starts_with('[') && v.ends_with(']') {
            Val::List(v[1..v.len() - 1].split(',').map(unquote).filter(|x| !x.is_empty()).collect())
        } else if v == "true" || v == "false" {
            Val::Bool(v == "true")
        } else {
            Val::Str(unquote(v))
        };
        out.insert(key, val);
    }
    out
}

fn needs_quotes(s: &str) -> bool {
    s.is_empty()
        || s.contains(": ")
        || s.starts_with(['[', '{', '*', '&', '!', '|', '>', '\'', '"', '%', '@', '#'])
        || s.trim() != s
}

pub fn quote(s: &str) -> String {
    if needs_quotes(s) { format!("\"{}\"", s.replace('"', "\\\"")) } else { s.to_string() }
}

/// Frontmatter schrijven (volgorde zoals gegeven).
pub fn emit(fields: &[(&str, Val)]) -> String {
    let mut out = String::from("---\n");
    for (k, v) in fields {
        match v {
            Val::Str(s) if s.contains('\n') => {
                out.push_str(&format!("{k}: |\n"));
                for l in s.lines() {
                    out.push_str(&format!("  {l}\n"));
                }
            }
            Val::Str(s) => out.push_str(&format!("{k}: {}\n", quote(s))),
            Val::Bool(b) => out.push_str(&format!("{k}: {b}\n")),
            Val::List(l) if l.is_empty() => {}
            Val::List(l) => {
                out.push_str(&format!("{k}: [{}]\n", l.iter().map(|x| quote(x)).collect::<Vec<_>>().join(", ")))
            }
        }
    }
    out.push_str("---\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shapes() {
        let (m, body) = split(
            "---\nname: pdf-tools\ndescription: \"Werk met PDF: splitsen\"\nglobs: [\"*.pdf\", docs/**]\nalwaysApply: false\nallowed-tools:\n  - Read\n  - Bash\nlong: >\n  een\n  twee\n---\n# Body\ntekst\n",
        );
        assert_eq!(m["name"].as_str(), "pdf-tools");
        assert_eq!(m["description"].as_str(), "Werk met PDF: splitsen");
        assert_eq!(m["globs"].as_list(), vec!["*.pdf", "docs/**"]);
        assert!(!m["alwaysApply"].as_bool());
        assert_eq!(m["allowed-tools"].as_list(), vec!["Read", "Bash"]);
        assert_eq!(m["long"].as_str(), "een twee");
        assert_eq!(body, "# Body\ntekst\n");
        let (m2, b2) = split("geen frontmatter");
        assert!(m2.is_empty() && b2 == "geen frontmatter");
        // Cursor schrijft globs vaak als kale komma-lijst.
        let (m3, _) = split("---\ndescription: x\nglobs: src/**/*.ts, *.tsx\nalwaysApply: true\n---\n");
        assert_eq!(m3["globs"].as_list(), vec!["src/**/*.ts", "*.tsx"]);
    }
}
