//! Structuur-bewust chunken: koppen (markdown), alinea's, codeblokken blijven heel; elke chunk
//! krijgt een locator (kop-pad) voor exacte bronverwijzing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub text: String,
    /// Kop-pad, bv. "Installatie > Windows".
    pub locator: String,
}

fn flush(out: &mut Vec<Chunk>, buf: &mut String, loc: &str) {
    let t = buf.trim();
    if !t.is_empty() {
        out.push(Chunk { text: t.to_string(), locator: loc.to_string() });
    }
    buf.clear();
}

/// Splits `text` in stukken van hooguit ~`max_chars` (codeblokken nooit gesplitst).
pub fn chunk(text: &str, max_chars: usize) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut heads: Vec<(usize, String)> = Vec::new();
    let mut buf = String::new();
    let mut in_code = false;
    let loc = |h: &Vec<(usize, String)>| h.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > ");
    let mut blocks: Vec<(String, String)> = Vec::new(); // (blok, locator)
    let mut cur = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            cur.push_str(line);
            cur.push('\n');
            if !in_code {
                blocks.push((std::mem::take(&mut cur), loc(&heads)));
            }
            continue;
        }
        if in_code {
            cur.push_str(line);
            cur.push('\n');
            continue;
        }
        if let Some(h) = trimmed.strip_prefix('#') {
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            let title = h.trim_start_matches('#').trim().to_string();
            if !cur.trim().is_empty() {
                blocks.push((std::mem::take(&mut cur), loc(&heads)));
            }
            heads.retain(|(l, _)| *l < level);
            heads.push((level, title));
            continue;
        }
        if trimmed.is_empty() {
            if !cur.trim().is_empty() {
                blocks.push((std::mem::take(&mut cur), loc(&heads)));
            }
            continue;
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        blocks.push((cur, loc(&heads)));
    }
    // Blokken samenvoegen tot ~max_chars per chunk, binnen dezelfde sectie.
    let mut buf_loc = String::new();
    for (b, l) in blocks {
        if (!buf.is_empty() && (buf.len() + b.len() > max_chars || l != buf_loc)) || b.len() > max_chars {
            flush(&mut out, &mut buf, &buf_loc);
        }
        buf_loc = l.clone();
        if b.len() > max_chars && !b.trim_start().starts_with("```") {
            // Te lange alinea: op zinnen splitsen.
            let mut part = String::new();
            for s in b.split_inclusive(['.', '!', '?']) {
                if part.len() + s.len() > max_chars && !part.is_empty() {
                    flush(&mut out, &mut part, &l);
                }
                part.push_str(s);
            }
            flush(&mut out, &mut part, &l);
            continue;
        }
        buf.push_str(&b);
        buf.push('\n');
    }
    flush(&mut out, &mut buf, &buf_loc);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_code_blocks_and_tracks_headings() {
        let t = "# Gids\nIntro.\n\n## Installatie\nStap een.\n\n```rust\nfn main() {}\n\nfn x() {}\n```\n\n## Gebruik\nKlaar.";
        let c = chunk(t, 1000);
        assert_eq!(c.len(), 3, "{c:?}");
        assert_eq!(c[0].locator, "Gids");
        assert_eq!(c[1].locator, "Gids > Installatie");
        assert!(c[1].text.contains("fn main() {}\n\nfn x() {}"), "codeblok heel");
        assert_eq!(c[2].locator, "Gids > Gebruik");
    }

    #[test]
    fn long_paragraph_is_split_on_sentences() {
        let t = "Zin een is hier. ".repeat(100);
        let c = chunk(&t, 300);
        assert!(c.len() > 3);
        assert!(c.iter().all(|x| x.text.len() <= 320));
    }

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk("Hallo", 1000).len(), 1);
        assert!(chunk("   ", 1000).is_empty());
    }
}
