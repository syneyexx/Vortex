//! Web: zoeken via SearXNG (lokaal, privé) en een pagina lezen (HTML → tekst).

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::{BuiltinConfig, s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::tool::{Tool, ToolCtx, ToolOutput};

pub fn tools(cfg: &BuiltinConfig) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(WebSearch::new(cfg)), Arc::new(WebFetch::new(cfg))]
}

pub(crate) fn client(cfg: &BuiltinConfig, timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(cfg.user_agent.clone().unwrap_or_else(|| "Vortex/0.1 (lokale assistent)".into()))
        .timeout(timeout)
        .build()
        .unwrap_or_default()
}

pub(crate) fn net_err(e: reqwest::Error) -> ToolError {
    if e.is_timeout() || e.is_connect() {
        ToolError::transient(e.to_string())
    } else {
        ToolError::failed(e.to_string())
    }
}

#[derive(Debug)]
pub struct WebSearch {
    c: ToolContract,
    base: Option<String>,
    http: reqwest::Client,
}
impl WebSearch {
    pub fn new(cfg: &BuiltinConfig) -> Self {
        Self {
            c: ToolContract::new(
                "web_search",
                "Zoek op het web via de lokale SearXNG; geeft titels, URL's en fragmenten.",
                Effect::Read,
                json!({"type": "object", "required": ["query"], "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 30, "default": 8},
                    "language": {"type": "string", "default": "nl"},
                    "time_range": {"type": "string", "enum": ["day", "week", "month", "year"]}
                }}),
            )
            .resources(vec![ResourceUse::Net])
            .net(vec![cfg.searxng_url.clone().unwrap_or_else(|| "searxng".into())])
            .cacheable()
            .timeout(20_000),
            base: cfg.searxng_url.clone(),
            http: client(cfg, Duration::from_secs(15)),
        }
    }
}
#[async_trait::async_trait]
impl Tool for WebSearch {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let Some(base) = &self.base else {
            return Err(ToolError::new(ToolErrorKind::Precondition, "geen SearXNG ingesteld")
                .with_hint("zet [tools] searxng_url in config/tools.toml of gebruik web_fetch met een bekende URL"));
        };
        let mut q = vec![
            ("q", s(&i, "query").to_string()),
            ("format", "json".into()),
            ("language", s(&i, "language").to_string()),
        ];
        if let Some(t) = i.get("time_range").and_then(|t| t.as_str()) {
            q.push(("time_range", t.into()));
        }
        let r =
            self.http.get(format!("{}/search", base.trim_end_matches('/'))).query(&q).send().await.map_err(net_err)?;
        if !r.status().is_success() {
            let st = r.status();
            return Err(if st.is_server_error() || st.as_u16() == 429 {
                ToolError::transient(format!("SearXNG: {st}"))
            } else {
                ToolError::failed(format!("SearXNG: {st}"))
            });
        }
        let v: serde_json::Value = r.json().await.map_err(net_err)?;
        let max = u(&i, "max_results").unwrap_or(8) as usize;
        let results: Vec<serde_json::Value> = v["results"]
            .as_array()
            .map(|a| {
                a.iter()
                    .take(max)
                    .map(|x| json!({"title": x["title"], "url": x["url"], "snippet": x["content"], "engine": x["engine"], "date": x["publishedDate"]}))
                    .collect()
            })
            .unwrap_or_default();
        let text = if results.is_empty() {
            "Geen resultaten.".to_string()
        } else {
            results
                .iter()
                .enumerate()
                .map(|(k, r)| format!("{}. {}\n   {}\n   {}", k + 1, s(r, "title"), s(r, "url"), s(r, "snippet")))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::json(json!({"results": results})).with_text(text))
    }
}

#[derive(Debug)]
pub struct WebFetch {
    c: ToolContract,
    http: reqwest::Client,
}
impl WebFetch {
    pub fn new(cfg: &BuiltinConfig) -> Self {
        Self {
            c: ToolContract::new(
                "web_fetch",
                "Lees een webpagina (HTML wordt leesbare tekst met koppen en links); ook JSON/tekst.",
                Effect::Read,
                json!({"type": "object", "required": ["url"], "properties": {
                    "url": {"type": "string", "minLength": 8},
                    "max_chars": {"type": "integer", "minimum": 500, "maximum": 200000, "default": 20000}
                }}),
            )
            .resources(vec![ResourceUse::Net])
            .net(vec!["*".into()])
            .cacheable()
            .timeout(30_000),
            http: client(cfg, Duration::from_secs(25)),
        }
    }
}
#[async_trait::async_trait]
impl Tool for WebFetch {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let url = s(&i, "url");
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ToolError::invalid("URL moet met http:// of https:// beginnen"));
        }
        let r = self.http.get(url).send().await.map_err(net_err)?;
        let st = r.status();
        if !st.is_success() {
            return Err(if st.is_server_error() || st.as_u16() == 429 {
                ToolError::transient(format!("{st}"))
            } else {
                ToolError::failed(format!("HTTP {st}"))
            });
        }
        let final_url = r.url().to_string();
        let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        let bytes = r.bytes().await.map_err(net_err)?;
        let bytes = &bytes[..bytes.len().min(4 << 20)];
        let raw = String::from_utf8_lossy(bytes);
        let (title, body) = if ct.contains("html") || raw.trim_start().starts_with('<') {
            html_to_text(&raw)
        } else if ct.contains("json") {
            (
                String::new(),
                serde_json::from_str::<serde_json::Value>(&raw)
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                    .unwrap_or_else(|_| raw.to_string()),
            )
        } else {
            (String::new(), raw.to_string())
        };
        let max = u(&i, "max_chars").unwrap_or(20_000) as usize;
        let truncated = body.chars().count() > max;
        let text: String = body.chars().take(max).collect();
        Ok(ToolOutput {
            data: json!({"url": final_url, "title": title, "content_type": ct, "chars": body.chars().count(), "truncated": truncated}),
            text: format!(
                "{}{}\n{}\n{text}{}",
                if title.is_empty() { "" } else { "# " },
                title,
                final_url,
                if truncated { "\n… [afgekapt]" } else { "" }
            ),
            preview: None,
        })
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|e| *e <= 10);
        match end {
            Some(e) => {
                let ent = &tail[1..e];
                let rep = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" | "#39" => Some('\''),
                    "nbsp" => Some(' '),
                    "euro" => Some('€'),
                    "hellip" => Some('…'),
                    "mdash" => Some('—'),
                    "ndash" => Some('–'),
                    _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                        u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
                    }
                    _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                match rep {
                    Some(c) => {
                        out.push(c);
                        rest = &tail[e + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// HTML → (titel, leesbare tekst). Scripts/styles weg, koppen/lijsten/links behouden.
pub fn html_to_text(html: &str) -> (String, String) {
    let lower = html.to_lowercase();
    let title = lower
        .find("<title")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .and_then(|s| lower[s..].find("</title>").map(|e| decode_entities(html[s..s + e].trim())))
        .unwrap_or_default();
    // Begin bij <main>/<article> als die er zijn (minder navigatie-ruis).
    let start = ["<main", "<article", "<body"].iter().find_map(|t| lower.find(t)).unwrap_or(0);
    let src = &html[start..];
    let src_l = &lower[start..];
    let mut out = String::new();
    let mut i = 0;
    let bytes = src.as_bytes();
    let mut href_stack: Vec<String> = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let Some(rel_end) = src_l[i..].find('>') else { break };
            let tag_full = &src_l[i + 1..i + rel_end];
            let orig_full = &src[i + 1..i + rel_end];
            let name: String =
                tag_full.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let closing = tag_full.starts_with('/');
            // Blokken overslaan.
            if !closing
                && ["script", "style", "noscript", "svg", "template", "head", "nav", "footer"].contains(&name.as_str())
            {
                let close = format!("</{name}");
                match src_l[i..].find(&close) {
                    Some(j) => {
                        let after = src_l[i + j..].find('>').map_or(src.len(), |k| i + j + k + 1);
                        i = after;
                        continue;
                    }
                    None => break,
                }
            }
            if tag_full.starts_with("!--") {
                i = src_l[i..].find("-->").map_or(src.len(), |k| i + k + 3);
                continue;
            }
            match (name.as_str(), closing) {
                ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                    let lvl = name[1..].parse::<usize>().unwrap_or(2);
                    out.push_str(&format!("\n\n{} ", "#".repeat(lvl)));
                }
                ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true)
                | ("p" | "div" | "section" | "tr" | "table" | "ul" | "ol" | "blockquote" | "pre", _) => {
                    out.push_str("\n\n")
                }
                ("br", _) => out.push('\n'),
                ("li", false) => out.push_str("\n- "),
                ("td" | "th", false) => out.push_str(" | "),
                ("a", false) => {
                    let href = orig_full
                        .split_whitespace()
                        .find_map(|a| {
                            a.strip_prefix("href=").map(|h| h.trim_matches(|c| c == '"' || c == '\'').to_string())
                        })
                        .unwrap_or_default();
                    href_stack.push(href);
                }
                ("a", true) => {
                    if let Some(h) = href_stack.pop()
                        && h.starts_with("http")
                    {
                        out.push_str(&format!(" ({h})"));
                    }
                }
                _ => {}
            }
            i += rel_end + 1;
        } else {
            let next = src[i..].find('<').map_or(src.len(), |k| i + k);
            out.push_str(&decode_entities(&src[i..next]));
            i = next;
        }
    }
    // Witruimte opruimen.
    let mut clean = String::new();
    let mut blank = 0;
    for line in out.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            blank += 1;
            if blank == 1 && !clean.is_empty() {
                clean.push('\n');
            }
        } else {
            blank = 0;
            clean.push_str(&l);
            clean.push('\n');
        }
    }
    (title, clean.trim().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_cleanup() {
        let (t, x) = super::html_to_text(
            "<html><head><title>Test &amp; zo</title><script>var a=1;</script></head><body><nav>menu</nav><h1>Kop</h1><p>Hallo&nbsp;<b>wereld</b> <a href=\"https://x.nl\">link</a></p><ul><li>een</li><li>twee</li></ul><!-- weg --></body></html>",
        );
        assert_eq!(t, "Test & zo");
        assert!(x.contains("# Kop"), "{x}");
        assert!(x.contains("Hallo wereld link (https://x.nl)"), "{x}");
        assert!(x.contains("- een\n- twee"), "{x}");
        assert!(!x.contains("var a") && !x.contains("menu") && !x.contains("weg"), "{x}");
    }
}
