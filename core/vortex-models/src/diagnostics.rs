//! Logprob-diagnostiek (W17.08): per token kans + alternatieven → gemiddelde log-kans,
//! perplexiteit, entropie, en onzekere stukken (tokens met lage kans of hoge entropie) zodat
//! de GUI kan laten zien waar het model twijfelde.
// VX:W17.08

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenLogprob {
    pub token: String,
    pub logprob: f32,
    #[serde(default)]
    pub top: Vec<(String, f32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub text: String,
    pub min_prob: f32,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub tokens: usize,
    pub mean_logprob: f32,
    pub perplexity: f32,
    pub mean_entropy: f32,
    /// Aandeel tokens onder de drempel.
    pub uncertain_share: f32,
    pub spans: Vec<Span>,
}

fn entropy(t: &TokenLogprob) -> f32 {
    if t.top.is_empty() {
        return -t.logprob.exp() * t.logprob;
    }
    let ps: Vec<f32> = t.top.iter().map(|(_, l)| l.exp()).collect();
    let rest = (1.0 - ps.iter().sum::<f32>()).max(0.0);
    ps.iter().chain(std::iter::once(&rest)).filter(|p| **p > 0.0).map(|p| -p * p.ln()).sum()
}

/// `low` = kansdrempel (bv. 0,35): aaneengesloten tokens eronder vormen één span.
pub fn diagnose(toks: &[TokenLogprob], low: f32) -> Diagnosis {
    if toks.is_empty() {
        return Diagnosis::default();
    }
    let n = toks.len() as f32;
    let mean = toks.iter().map(|t| t.logprob).sum::<f32>() / n;
    let mut spans: Vec<Span> = Vec::new();
    let mut pos = 0usize;
    let mut cur: Option<Span> = None;
    let mut unsure = 0usize;
    for t in toks {
        let p = t.logprob.exp();
        if p < low {
            unsure += 1;
            let s = cur.get_or_insert_with(|| Span { start: pos, min_prob: 1.0, ..Default::default() });
            s.text.push_str(&t.token);
            s.min_prob = s.min_prob.min(p);
            s.alternatives.extend(t.top.iter().filter(|(a, _)| a != &t.token).take(2).map(|(a, _)| a.clone()));
        } else if let Some(s) = cur.take() {
            spans.push(s);
        }
        pos += t.token.len();
    }
    spans.extend(cur);
    Diagnosis {
        tokens: toks.len(),
        mean_logprob: mean,
        perplexity: (-mean).exp(),
        mean_entropy: toks.iter().map(entropy).sum::<f32>() / n,
        uncertain_share: unsure as f32 / n,
        spans,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_uncertain_span() {
        let t = |s: &str, p: f32| TokenLogprob {
            token: s.into(),
            logprob: p.ln(),
            top: vec![(s.into(), p.ln()), ("x".into(), (1.0 - p).ln())],
        };
        let d = diagnose(&[t("De", 0.99), t(" hoofdstad", 0.95), t(" is", 0.97), t(" Lyon", 0.2), t(".", 0.9)], 0.35);
        assert_eq!(d.spans.len(), 1);
        assert_eq!(d.spans[0].text, " Lyon");
        assert_eq!(d.spans[0].alternatives, vec!["x".to_string()]);
        assert!(d.perplexity > 1.0 && (d.uncertain_share - 0.2).abs() < 1e-6);
    }
}
