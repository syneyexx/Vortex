//! Reader/Extractor: HTML → titel, hoofdtekst (koppen/lijsten), links met ankertekst.

use reqwest::Url;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extracted {
    pub title: String,
    pub text: String,
    pub links: Vec<(String, String)>,
}

fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let l = tag.to_lowercase();
    let i = l.find(&format!("{name}="))?;
    let rest = &tag[i + name.len() + 1..];
    let (q, rest) = match rest.chars().next()? {
        c @ ('"' | '\'') => (c, &rest[1..]),
        _ => (' ', rest),
    };
    let end = if q == ' ' { rest.find([' ', '>']).unwrap_or(rest.len()) } else { rest.find(q)? };
    Some(decode(&rest[..end]))
}

/// Extractie met basis-URL voor relatieve links. Navigatie/scripts weg; `<main>`/`<article>` voorkeur.
pub fn extract(html: &str, base: &Url) -> Extracted {
    let lower = html.to_lowercase();
    let title = lower
        .find("<title")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .and_then(|s| lower[s..].find("</title>").map(|e| decode(html[s..s + e].trim())))
        .unwrap_or_default();
    let start = ["<main", "<article", "<body"].iter().find_map(|t| lower.find(t)).unwrap_or(0);
    let (src, srcl) = (&html[start..], &lower[start..]);
    let mut text = String::new();
    let mut links = Vec::new();
    let mut i = 0;
    let mut anchor: Option<(String, String)> = None;
    while i < src.len() {
        if src.as_bytes()[i] == b'<' {
            let Some(rel) = srcl[i..].find('>') else { break };
            let tag = &src[i + 1..i + rel];
            let tl = &srcl[i + 1..i + rel];
            let name: String = tl.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let closing = tl.starts_with('/');
            if !closing
                && ["script", "style", "noscript", "nav", "footer", "header", "aside", "svg", "form", "template"]
                    .contains(&name.as_str())
            {
                let close = format!("</{name}");
                i = srcl[i..]
                    .find(&close)
                    .map_or(src.len(), |j| srcl[i + j..].find('>').map_or(src.len(), |k| i + j + k + 1));
                continue;
            }
            if tl.starts_with("!--") {
                i = srcl[i..].find("-->").map_or(src.len(), |k| i + k + 3);
                continue;
            }
            match (name.as_str(), closing) {
                ("a", false) => {
                    if let Some(h) = attr(tag, "href") {
                        anchor = Some((h, String::new()));
                    }
                }
                ("a", true) => {
                    if let Some((h, t)) = anchor.take()
                        && let Ok(u) = base.join(&h)
                        && (u.scheme() == "http" || u.scheme() == "https")
                    {
                        let mut u = u;
                        u.set_fragment(None);
                        links.push((u.to_string(), t.split_whitespace().collect::<Vec<_>>().join(" ")));
                    }
                }
                ("h1" | "h2" | "h3" | "h4", false) => text.push_str("\n\n# "),
                ("p" | "div" | "li" | "tr" | "br" | "section" | "h1" | "h2" | "h3" | "h4" | "table" | "pre", _) => {
                    text.push('\n')
                }
                ("td" | "th", false) => text.push_str(" | "),
                _ => {}
            }
            i += rel + 1;
        } else {
            let next = src[i..].find('<').map_or(src.len(), |k| i + k);
            let t = decode(&src[i..next]);
            if let Some((_, a)) = anchor.as_mut() {
                a.push_str(&t);
                a.push(' ');
            }
            text.push_str(&t);
            i = next;
        }
    }
    let text = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    links.dedup();
    Extracted { title, text, links }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_and_resolves_links() {
        let base = Url::parse("https://x.nl/a/b.html").unwrap();
        let e = extract(
            "<html><head><title>T &amp; U</title></head><body><nav><a href='/menu'>Menu</a></nav><main><h1>Kop</h1><p>Tekst over <a href=\"c.html#s\">QLoRA training</a>.</p><a href='https://y.org/z'>Elders</a><a href='mailto:a@b'>mail</a></main></body></html>",
            &base,
        );
        assert_eq!(e.title, "T & U");
        assert!(e.text.contains("# Kop") && e.text.contains("Tekst over QLoRA training"), "{}", e.text);
        assert_eq!(
            e.links,
            vec![
                ("https://x.nl/a/c.html".to_string(), "QLoRA training".to_string()),
                ("https://y.org/z".into(), "Elders".into())
            ]
        );
        assert!(!e.text.contains("Menu"));
    }
}
