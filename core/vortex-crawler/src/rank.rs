//! Ranker (focused crawling): relevantie van een link voor de onderzoeksvraag +
//! value-of-information (nieuwe domeinen, niet te diep, betrouwbare bronnen). Curator-hulpen:
//! simhash voor near-duplicates en nieuwheid.

use std::collections::HashSet;

pub fn terms(s: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "de", "het", "een", "en", "van", "in", "op", "the", "of", "and", "to", "a", "is", "for", "with", "wat", "hoe",
        "welke",
    ];
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(String::from)
        .collect()
}

/// Score 0..1 voor een link.
pub fn score_link(query: &[String], url: &str, anchor: &str, depth: usize, domain_seen: bool, reliability: f64) -> f64 {
    let hay: HashSet<String> =
        terms(&format!("{anchor} {}", url.replace(['/', '-', '_', '.'], " "))).into_iter().collect();
    let hits = query.iter().filter(|q| hay.contains(*q)).count();
    let rel = if query.is_empty() { 0.3 } else { hits as f64 / query.len().min(5) as f64 };
    let mut s = 0.6 * rel.min(1.0) + 0.2 * reliability;
    if !domain_seen {
        s += 0.1; // diversiteit = onafhankelijke bronnen
    }
    s -= 0.08 * depth as f64;
    if url.ends_with(".pdf") {
        s += 0.05; // primaire documenten
    }
    s.clamp(0.0, 1.0)
}

/// Kenmerken voor de neurale ranker `crawl_rank` (volgorde = contract met `vortex-neural::spec`).
pub fn link_vector(
    query: &[String],
    url: &str,
    anchor: &str,
    depth: usize,
    domain_seen: bool,
    reliability: f64,
) -> Vec<f32> {
    let hay: HashSet<String> =
        terms(&format!("{anchor} {}", url.replace(['/', '-', '_', '.'], " "))).into_iter().collect();
    let hits = query.iter().filter(|q| hay.contains(*q)).count();
    let rel = if query.is_empty() { 0.3 } else { (hits as f64 / query.len().min(5) as f64).min(1.0) };
    let anchor_hits = terms(anchor).iter().filter(|w| query.contains(w)).count();
    vec![
        rel as f32,
        reliability as f32,
        if domain_seen { 0.0 } else { 1.0 },
        depth as f32 / 5.0,
        if url.ends_with(".pdf") { 1.0 } else { 0.0 },
        (anchor.split_whitespace().count() as f32 / 10.0).min(1.0),
        (anchor_hits as f32 / 3.0).min(1.0),
        (url.len() as f32 / 200.0).min(1.0),
        if url.contains('?') { 1.0 } else { 0.0 },
    ]
}

/// 64-bits simhash over woord-3-grammen.
pub fn simhash(text: &str) -> u64 {
    let w: Vec<&str> = text.split_whitespace().collect();
    let mut v = [0i32; 64];
    for g in w.windows(3) {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in g.join(" ").to_lowercase().bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for (i, x) in v.iter_mut().enumerate() {
            *x += if h >> i & 1 == 1 { 1 } else { -1 };
        }
    }
    v.iter().enumerate().fold(0u64, |acc, (i, x)| if *x > 0 { acc | 1 << i } else { acc })
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Aandeel nieuwe woord-3-grammen t.o.v. wat al gezien is (value-of-information van een pagina).
pub fn novelty(text: &str, seen: &mut HashSet<u64>) -> f64 {
    let w: Vec<&str> = text.split_whitespace().collect();
    let grams: Vec<u64> = w
        .windows(3)
        .map(|g| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in g.join(" ").to_lowercase().bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h
        })
        .collect();
    if grams.is_empty() {
        return 0.0;
    }
    let new = grams.iter().filter(|g| !seen.contains(g)).count();
    seen.extend(grams.iter().copied());
    new as f64 / grams.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_and_dedup() {
        let q = terms("QLoRA fine-tuning geheugengebruik");
        let a = score_link(&q, "https://x.org/qlora-geheugengebruik", "QLoRA geheugengebruik", 1, false, 0.5);
        let b = score_link(&q, "https://x.org/contact", "Contact", 1, true, 0.5);
        assert!(a > b + 0.3);
        let t1: String =
            (0..60).map(|i| format!("zin {i} gaat over het trainen van taalmodellen met lora. ")).collect();
        let t2 = t1.replacen("zin 30 gaat", "zin 30 ging", 1);
        let t3: String =
            (0..60).map(|i| format!("recept {i} met tomaten basilicum en verse pasta vandaag. ")).collect();
        assert!(hamming(simhash(&t1), simhash(&t2)) <= 3, "near-duplicate");
        assert!(hamming(simhash(&t1), simhash(&t3)) > 10);
        let mut seen = HashSet::new();
        assert!(novelty(&t1, &mut seen) > 0.99);
        assert!(novelty(&t1, &mut seen) < 0.01);
    }
}
