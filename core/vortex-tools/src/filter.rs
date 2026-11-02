//! Resultaatfiltering met code + tool result caching (W08.10).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::tool::ToolOutput;

/// Filter die het model (of de executor) op een resultaat kan zetten vóór het in de context gaat.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultFilter {
    /// JSON-pointer in `data` (bv. `/items/0`).
    #[serde(default)]
    pub pointer: Option<String>,
    /// Alleen regels die dit bevatten (hoofdletterongevoelig).
    #[serde(default)]
    pub grep: Option<String>,
    #[serde(default)]
    pub head: Option<usize>,
    #[serde(default)]
    pub tail: Option<usize>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

impl ResultFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn apply(&self, mut out: ToolOutput) -> ToolOutput {
        if let Some(p) = &self.pointer
            && let Some(v) = out.data.pointer(p).cloned()
        {
            out = ToolOutput::json(v);
        }
        let mut lines: Vec<&str> = out.text.lines().collect();
        if let Some(g) = &self.grep {
            let g = g.to_lowercase();
            lines.retain(|l| l.to_lowercase().contains(&g));
        }
        if let Some(h) = self.head {
            lines.truncate(h);
        }
        if let Some(t) = self.tail {
            let n = lines.len();
            lines = lines.split_off(n.saturating_sub(t));
        }
        let mut text = lines.join("\n");
        if let Some(m) = self.max_chars {
            text = trim_middle(&text, m);
        }
        out.text = text;
        out
    }
}

/// Lange uitvoer: begin + eind houden, midden weglaten (met melding).
pub fn trim_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max * 2 / 3).collect();
    let tail: String = s.chars().skip(n - max / 3).collect();
    format!("{head}\n… [{} tekens weggelaten — vraag gericht op met filter/offset] …\n{tail}", n - max)
}

type CacheInner = (HashMap<String, (Instant, ToolOutput)>, VecDeque<String>);

/// TTL-cache voor resultaten van leestools (sleutel = tool + canonieke invoer).
#[derive(Debug)]
pub struct ToolCache {
    inner: Mutex<CacheInner>,
    ttl: Duration,
    max: usize,
}

impl ToolCache {
    pub fn new(ttl: Duration, max: usize) -> Self {
        Self { inner: Mutex::new((HashMap::new(), VecDeque::new())), ttl, max }
    }
    pub fn get(&self, key: &str) -> Option<ToolOutput> {
        let g = self.inner.lock();
        g.0.get(key).filter(|(t, _)| t.elapsed() < self.ttl).map(|(_, o)| o.clone())
    }
    pub fn put(&self, key: String, out: ToolOutput) {
        let mut g = self.inner.lock();
        if !g.0.contains_key(&key) {
            g.1.push_back(key.clone());
        }
        g.0.insert(key, (Instant::now(), out));
        while g.1.len() > self.max {
            if let Some(old) = g.1.pop_front() {
                g.0.remove(&old);
            }
        }
    }
    pub fn clear(&self) {
        let mut g = self.inner.lock();
        g.0.clear();
        g.1.clear();
    }
    pub fn len(&self) -> usize {
        self.inner.lock().0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_lines_and_trims() {
        let out = ToolOutput::text("a fout\nb ok\nc FOUT\nd");
        let f = ResultFilter { grep: Some("fout".into()), ..Default::default() };
        assert_eq!(f.apply(out.clone()).text, "a fout\nc FOUT");
        let f = ResultFilter { tail: Some(2), ..Default::default() };
        assert_eq!(f.apply(out).text, "c FOUT\nd");
        let t = trim_middle(&"x".repeat(1000), 100);
        assert!(t.contains("900 tekens weggelaten"));
    }

    #[test]
    fn cache_ttl_and_cap() {
        let c = ToolCache::new(Duration::from_secs(60), 2);
        c.put("a".into(), ToolOutput::text("1"));
        c.put("b".into(), ToolOutput::text("2"));
        c.put("c".into(), ToolOutput::text("3"));
        assert!(c.get("a").is_none());
        assert_eq!(c.get("c").unwrap().text, "3");
    }
}
