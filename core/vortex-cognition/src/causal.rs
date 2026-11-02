//! Causaal en counterfactual redeneren (W18.03) + wereldmodel-simulaties (W18.04).
//! Graaf van oorzaak → gevolg met kans per verband (noisy-OR): effecten van een ingreep
//! (do(X)), counterfactual ("wat als X niet was gebeurd": X uit de graaf, verschil per gevolg),
//! Monte-Carlo-simulaties van scenario's, en het herkennen van causale claims in tekst (tracking).
// VX:W18.03

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub fn norm(s: &str) -> String {
    s.trim().trim_end_matches(['.', ',', '!', '?']).to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CausalGraph {
    /// (oorzaak, gevolg) → kans dat het verband "vuurt".
    pub edges: BTreeMap<(String, String), f64>,
}

impl CausalGraph {
    pub fn add(&mut self, cause: &str, effect: &str, p: f64) {
        let k = (norm(cause), norm(effect));
        if k.0 != k.1 {
            let e = self.edges.entry(k).or_insert(0.0);
            *e = e.max(p.clamp(0.0, 1.0));
        }
    }

    fn children(&self, n: &str) -> Vec<(&str, f64)> {
        self.edges.iter().filter(|((c, _), _)| c == n).map(|((_, e), p)| (e.as_str(), *p)).collect()
    }

    fn parents(&self, n: &str) -> Vec<(&str, f64)> {
        self.edges.iter().filter(|((_, e), _)| e == n).map(|((c, _), p)| (c.as_str(), *p)).collect()
    }

    fn nodes(&self) -> HashSet<String> {
        self.edges.keys().flat_map(|(a, b)| [a.clone(), b.clone()]).collect()
    }

    /// Topologische volgorde (cycli worden doorbroken: knopen in een cyclus achteraan).
    fn order(&self) -> Vec<String> {
        let nodes = self.nodes();
        let mut indeg: HashMap<String, usize> = nodes.iter().map(|n| (n.clone(), 0)).collect();
        for (_, e) in self.edges.keys() {
            *indeg.entry(e.clone()).or_default() += 1;
        }
        let mut q: Vec<String> = indeg.iter().filter(|(_, d)| **d == 0).map(|(n, _)| n.clone()).collect();
        q.sort();
        let mut out = Vec::new();
        while let Some(n) = q.pop() {
            out.push(n.clone());
            for (c, _) in self.children(&n) {
                if let Some(d) = indeg.get_mut(c) {
                    *d -= 1;
                    if *d == 0 {
                        q.push(c.to_string());
                    }
                }
            }
        }
        let rest: Vec<String> = nodes.into_iter().filter(|n| !out.contains(n)).collect();
        out.extend(rest);
        out
    }

    /// Kans per knoop gegeven vaste waarden (evidence/ingrepen), noisy-OR, analytisch.
    pub fn propagate(&self, fixed: &HashMap<String, f64>) -> BTreeMap<String, f64> {
        let mut p: BTreeMap<String, f64> = BTreeMap::new();
        for n in self.order() {
            let v = match fixed.get(&n) {
                Some(f) => *f,
                None => {
                    let parents = self.parents(&n);
                    if parents.is_empty() {
                        0.0
                    } else {
                        1.0 - parents.iter().map(|(c, w)| 1.0 - w * p.get(*c).copied().unwrap_or(0.0)).product::<f64>()
                    }
                }
            };
            p.insert(n, v);
        }
        p
    }

    /// do(X = 1): verwachte gevolgen (alleen knopen die stroomafwaarts liggen).
    pub fn effects_of(&self, x: &str, evidence: &[String]) -> Vec<(String, f64)> {
        let x = norm(x);
        let mut fixed: HashMap<String, f64> = evidence.iter().map(|e| (norm(e), 1.0)).collect();
        fixed.insert(x.clone(), 1.0);
        let down = self.downstream(&x);
        let mut v: Vec<(String, f64)> = self.propagate(&fixed).into_iter().filter(|(n, _)| down.contains(n)).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

    fn downstream(&self, x: &str) -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut st = vec![x.to_string()];
        while let Some(n) = st.pop() {
            for (c, _) in self.children(&n) {
                if seen.insert(c.to_string()) {
                    st.push(c.to_string());
                }
            }
        }
        seen
    }

    /// Counterfactual: wat als `x` NIET was gebeurd (gegeven de rest van de evidence)?
    /// Per gevolg: kans mét x, kans zónder x, verschil.
    pub fn counterfactual(&self, x: &str, evidence: &[String]) -> Vec<(String, f64, f64)> {
        let x = norm(x);
        let base: HashMap<String, f64> = evidence.iter().map(|e| (norm(e), 1.0)).filter(|(k, _)| *k != x).collect();
        let mut with = base.clone();
        with.insert(x.clone(), 1.0);
        let mut without = base;
        without.insert(x.clone(), 0.0);
        let (a, b) = (self.propagate(&with), self.propagate(&without));
        let down = self.downstream(&x);
        let mut v: Vec<(String, f64, f64)> = down
            .into_iter()
            .map(|n| (n.clone(), a.get(&n).copied().unwrap_or(0.0), b.get(&n).copied().unwrap_or(0.0)))
            .collect();
        v.sort_by(|p, q| (q.1 - q.2).total_cmp(&(p.1 - p.2)));
        v
    }

    /// W18.04: Monte Carlo — `runs` scenario's met ingrepen (aan) en blokkades (uit); per knoop
    /// de frequentie, plus de vaakste gezamenlijke uitkomsten.
    // VX:W18.04
    pub fn simulate(&self, on: &[String], off: &[String], runs: usize, seed: u64) -> Simulation {
        let order = self.order();
        let on: HashSet<String> = on.iter().map(|s| norm(s)).collect();
        let off: HashSet<String> = off.iter().map(|s| norm(s)).collect();
        let mut s = seed.max(1);
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % 1_000_000) as f64 / 1_000_000.0
        };
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut outcomes: HashMap<Vec<String>, usize> = HashMap::new();
        for _ in 0..runs.max(1) {
            let mut state: HashSet<String> = HashSet::new();
            for n in &order {
                let v = if off.contains(n) {
                    false
                } else if on.contains(n) {
                    true
                } else {
                    self.parents(n).iter().any(|(c, w)| state.contains(*c) && rnd() < *w)
                };
                if v {
                    state.insert(n.clone());
                    *counts.entry(n.clone()).or_default() += 1;
                }
            }
            let mut key: Vec<String> = state.into_iter().filter(|n| !on.contains(n)).collect();
            key.sort();
            *outcomes.entry(key).or_default() += 1;
        }
        let mut top: Vec<(Vec<String>, f64)> =
            outcomes.into_iter().map(|(k, c)| (k, c as f64 / runs.max(1) as f64)).collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(5);
        Simulation {
            runs,
            frequencies: counts.into_iter().map(|(k, c)| (k, c as f64 / runs.max(1) as f64)).collect(),
            scenarios: top,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simulation {
    pub runs: usize,
    pub frequencies: BTreeMap<String, f64>,
    /// Meest voorkomende uitkomsten (verzameling gevolgen) met kans.
    pub scenarios: Vec<(Vec<String>, f64)>,
}

/// Causale claims in tekst (NL + EN): "X leidt tot Y", "X veroorzaakt Y", "Y door X", "Y omdat X",
/// "X zorgt voor Y", "because of X, Y" … → (oorzaak, gevolg). Voor tracking van eigen redeneringen.
pub fn extract_claims(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for sent in text.split(['.', '!', '?', '\n', ';']) {
        let s = sent.trim();
        if s.is_empty() {
            continue;
        }
        let l = s.to_lowercase();
        let pick = |a: &str, b: &str| -> Option<(String, String)> {
            let (a, b) = (a.trim().trim_matches(','), b.trim().trim_matches(','));
            let ok = |x: &str| (2..=12).contains(&x.split_whitespace().count());
            (ok(a) && ok(b)).then(|| (norm(a), norm(b)))
        };
        let forward = [
            " leidt tot ",
            " leiden tot ",
            " veroorzaakt ",
            " veroorzaken ",
            " zorgt voor ",
            " zorgen voor ",
            " resulteert in ",
            " leads to ",
            " causes ",
            " results in ",
        ];
        let backward = [" omdat ", " doordat ", " vanwege ", " because of ", " because ", " due to ", " door "];
        if let Some(c) =
            forward.iter().find_map(|m| l.find(m).map(|i| (i, m.len()))).and_then(|(i, n)| pick(&l[..i], &l[i + n..]))
        {
            out.push(c);
        } else if let Some(c) =
            backward.iter().find_map(|m| l.find(m).map(|i| (i, m.len()))).and_then(|(i, n)| pick(&l[i + n..], &l[..i]))
        {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> CausalGraph {
        let mut g = CausalGraph::default();
        g.add("renteverhoging", "minder hypotheken", 0.8);
        g.add("minder hypotheken", "dalende huizenprijzen", 0.6);
        g.add("woningtekort", "stijgende huizenprijzen", 0.9);
        g.add("dalende huizenprijzen", "minder bouw", 0.5);
        g
    }

    #[test]
    fn effects_counterfactual_simulation() {
        let g = g();
        let e = g.effects_of("Renteverhoging", &[]);
        assert_eq!(e[0], ("minder hypotheken".to_string(), 0.8));
        assert!((e.iter().find(|x| x.0 == "dalende huizenprijzen").map(|x| x.1).unwrap_or(0.0) - 0.48).abs() < 1e-9);
        assert!(!e.iter().any(|x| x.0 == "stijgende huizenprijzen"), "niet stroomafwaarts");
        let cf = g.counterfactual("renteverhoging", &[]);
        let d = cf.iter().find(|x| x.0 == "dalende huizenprijzen").map(|x| x.1 - x.2).unwrap_or(0.0);
        assert!((d - 0.48).abs() < 1e-9);
        let sim = g.simulate(&["renteverhoging".into()], &[], 4000, 42);
        assert!((sim.frequencies["dalende huizenprijzen"] - 0.48).abs() < 0.03, "{sim:?}");
        let blocked = g.simulate(&["renteverhoging".into()], &["minder hypotheken".into()], 1000, 42);
        assert!(!blocked.frequencies.contains_key("dalende huizenprijzen"), "blokkade werkt");
    }

    #[test]
    fn extracts_causal_claims() {
        let c = extract_claims(
            "Hogere rente leidt tot minder hypotheken. De huizenprijzen dalen omdat er minder hypotheken zijn. Dit is los.",
        );
        assert_eq!(c[0], ("hogere rente".into(), "minder hypotheken".into()));
        assert_eq!(c[1], ("er minder hypotheken zijn".into(), "de huizenprijzen dalen".into()));
        assert_eq!(c.len(), 2);
    }
}
