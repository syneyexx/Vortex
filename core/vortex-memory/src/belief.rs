//! Experiment (W12.09, op verzoek): kwantum-achtige kansmodellen naast gewone Bayes voor
//! onzekerheid en tegenstrijdige bronnen.
//!
//! * Bayes: posterior ∝ prior × Π P(bewijs | H) — volgorde doet er niet toe; kopieën van
//!   dezelfde bron tellen elke keer opnieuw mee.
//! * Kwantum-achtig: geloof = amplitudevector ψ (√p). Bewijs = gedeeltelijke projectie
//!   M = I − λ(I − |v⟩⟨v|) met |v⟩ ∝ R(θ_context)·√L. Projecties met verschillende contexthoek
//!   commuteren niet → volgorde-effecten en interferentie; herhaald identiek bewijs verzadigt
//!   (projectie is idempotent) in plaats van te stapelen.
//!
//! [`experiment`] vergelijkt beide (nauwkeurigheid en Brier-score) op synthetische
//! conflict-scenario's: onafhankelijke bronnen, gekopieerde bronnen (afhankelijkheid) en
//! verouderde informatie (recency). θ en λ worden op een trainsplit gekozen; uitslag eerlijk.
// VX:W12.09

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// P(rapport | H_i) per hypothese.
    pub likelihood: Vec<f64>,
    /// Soort bron (context van de "meting").
    pub context: usize,
}

fn norm(p: &mut [f64]) {
    let s: f64 = p.iter().sum();
    if s > 0.0 {
        p.iter_mut().for_each(|x| *x /= s);
    }
}

pub fn bayes(prior: &[f64], ev: &[Evidence]) -> Vec<f64> {
    let mut p = prior.to_vec();
    for e in ev {
        for (x, l) in p.iter_mut().zip(&e.likelihood) {
            *x *= l.max(1e-9);
        }
        norm(&mut p);
    }
    p
}

/// Draai de eerste twee assen over hoek `t` (Givens) — de "meetcontext".
fn rotate(v: &mut [f64], t: f64) {
    if v.len() >= 2 && t != 0.0 {
        let (c, s) = (t.cos(), t.sin());
        let (a, b) = (v[0], v[1]);
        v[0] = c * a - s * b;
        v[1] = s * a + c * b;
    }
}

pub fn quantum(prior: &[f64], ev: &[Evidence], theta: &[f64], lambda: f64) -> Vec<f64> {
    let mut psi: Vec<f64> = prior.iter().map(|p| p.max(0.0).sqrt()).collect();
    for e in ev {
        let mut v: Vec<f64> = e.likelihood.iter().map(|l| l.max(1e-9).sqrt()).collect();
        rotate(&mut v, theta.get(e.context).copied().unwrap_or(0.0));
        let n = v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
        v.iter_mut().for_each(|x| *x /= n);
        let dot: f64 = v.iter().zip(&psi).map(|(a, b)| a * b).sum();
        // ψ' = (1−λ)ψ + λ⟨v|ψ⟩v
        for (x, vi) in psi.iter_mut().zip(&v) {
            *x = (1.0 - lambda) * *x + lambda * dot * vi;
        }
        let n = psi.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
        psi.iter_mut().for_each(|x| *x /= n);
    }
    psi.iter().map(|x| x * x).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub n: usize,
    pub bayes_acc: f64,
    pub quantum_acc: f64,
    pub bayes_brier: f64,
    pub quantum_brier: f64,
    /// Gemiddeld verschil P(A) bij omgekeerde volgorde van het bewijs (kwantum; Bayes = 0).
    pub order_effect: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ABReport {
    pub scenarios: Vec<ScenarioResult>,
    pub theta: Vec<f64>,
    pub lambda: f64,
    pub verdict: String,
}

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Eén geval: bewijs + waarheid (A=0/B=1).
type Case = (Vec<Evidence>, usize);

/// Eén scenario: (bewijs, waarheid A=0/B=1).
fn scenario(kind: &str, r: &mut Rng) -> (Vec<Evidence>, usize) {
    let truth = usize::from(r.f() < 0.5);
    let report = |r: &mut Rng, rel: f64, ctx: usize, truth: usize| -> Evidence {
        let says = if r.f() < rel { truth } else { 1 - truth };
        let mut l = vec![1.0 - rel, 1.0 - rel];
        l[says] = rel;
        Evidence { likelihood: l, context: ctx }
    };
    let mut ev = Vec::new();
    match kind {
        // Onafhankelijke bronnen van wisselende betrouwbaarheid.
        "onafhankelijk" => {
            for i in 0..4 {
                let rel = 0.6 + 0.3 * r.f();
                ev.push(report(r, rel, i % 3, truth));
            }
        }
        // Eén onbetrouwbare bron vaak gekopieerd (zelfde context) + één betrouwbare onafhankelijke.
        "kopieën" => {
            let rel = 0.55;
            let base = report(r, rel, 1, truth);
            for _ in 0..4 {
                ev.push(base.clone());
            }
            ev.push(report(r, 0.85, 0, truth));
        }
        // Feit veranderd: oude bronnen (context 2) zeggen de oude waarde, nieuwe bron de nieuwe.
        _ => {
            for _ in 0..3 {
                let mut l = vec![0.3, 0.3];
                l[1 - truth] = 0.75;
                ev.push(Evidence { likelihood: l, context: 2 });
            }
            ev.push(report(r, 0.85, 0, truth));
        }
    }
    (ev, truth)
}

fn score(data: &[(Vec<Evidence>, usize)], f: &dyn Fn(&[Evidence]) -> Vec<f64>) -> (f64, f64) {
    let (mut acc, mut brier) = (0.0, 0.0);
    for (ev, t) in data {
        let p = f(ev);
        if (p[0] < p[1]) == (*t == 1) {
            acc += 1.0;
        }
        let pt = p[*t];
        brier += (1.0 - pt).powi(2);
    }
    let n = data.len().max(1) as f64;
    (acc / n, brier / n)
}

/// A/B-experiment. `n` scenario's per soort (train = eerste helft, test = tweede helft).
pub fn experiment(n: usize, seed: u64) -> ABReport {
    let mut r = Rng(seed);
    let kinds = ["onafhankelijk", "kopieën", "verouderd"];
    let data: Vec<(&str, Vec<Case>)> =
        kinds.iter().map(|k| (*k, (0..n).map(|_| scenario(k, &mut r)).collect())).collect();
    let prior = [0.5, 0.5];
    // θ per context en λ kiezen op de trainhelft (rooster), over alle soorten samen.
    let grid_t = [0.0, 0.15, 0.3, 0.5];
    let grid_l = [0.5, 0.7, 0.9];
    let mut best = (f64::MAX, vec![0.0; 3], 0.9);
    for t0 in grid_t {
        for t1 in grid_t {
            for t2 in grid_t {
                for l in grid_l {
                    let th = vec![t0, t1, t2];
                    let b: f64 = data.iter().map(|(_, d)| score(&d[..n / 2], &|e| quantum(&prior, e, &th, l)).1).sum();
                    if b < best.0 {
                        best = (b, th, l);
                    }
                }
            }
        }
    }
    let (_, theta, lambda) = best;
    let mut rep = ABReport { theta: theta.clone(), lambda, ..Default::default() };
    for (name, d) in &data {
        let test = &d[n / 2..];
        let (ba, bb) = score(test, &|e| bayes(&prior, e));
        let (qa, qb) = score(test, &|e| quantum(&prior, e, &theta, lambda));
        let oe = test
            .iter()
            .map(|(e, _)| {
                let mut rev = e.clone();
                rev.reverse();
                (quantum(&prior, e, &theta, lambda)[0] - quantum(&prior, &rev, &theta, lambda)[0]).abs()
            })
            .sum::<f64>()
            / test.len().max(1) as f64;
        rep.scenarios.push(ScenarioResult {
            name: (*name).into(),
            n: test.len(),
            bayes_acc: ba,
            quantum_acc: qa,
            bayes_brier: bb,
            quantum_brier: qb,
            order_effect: oe,
        });
    }
    let qwins = rep
        .scenarios
        .iter()
        .filter(|s| s.quantum_brier < s.bayes_brier - 0.005)
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    let bwins = rep
        .scenarios
        .iter()
        .filter(|s| s.bayes_brier < s.quantum_brier - 0.005)
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    rep.verdict = format!(
        "Kwantum-achtig beter (Brier) bij: {}; Bayes beter bij: {}.",
        if qwins.is_empty() { "geen".into() } else { qwins.join(", ") },
        if bwins.is_empty() { "geen".into() } else { bwins.join(", ") }
    );
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantum_reduces_to_bayes_without_context_and_full_projection_is_idempotent() {
        let prior = [0.5, 0.5];
        let ev = vec![Evidence { likelihood: vec![0.8, 0.2], context: 0 }];
        let b = bayes(&prior, &ev);
        let q = quantum(&prior, &ev, &[0.0], 1.0);
        assert!((b[0] - q[0]).abs() < 1e-9, "{b:?} {q:?}");
        // Zelfde bewijs nogmaals: Bayes stapelt, volledige projectie niet.
        let ev2 = vec![ev[0].clone(), ev[0].clone()];
        assert!(bayes(&prior, &ev2)[0] > b[0] + 0.05);
        assert!((quantum(&prior, &ev2, &[0.0], 1.0)[0] - q[0]).abs() < 1e-9);
    }

    #[test]
    fn order_effects_only_with_context_angles() {
        let prior = [0.5, 0.5];
        let a = Evidence { likelihood: vec![0.8, 0.2], context: 0 };
        let b = Evidence { likelihood: vec![0.3, 0.7], context: 1 };
        let ab = quantum(&prior, &[a.clone(), b.clone()], &[0.0, 0.5], 0.8);
        let ba = quantum(&prior, &[b, a], &[0.0, 0.5], 0.8);
        assert!((ab[0] - ba[0]).abs() > 1e-3, "niet-commuterend");
    }

    #[test]
    fn experiment_runs_and_reports_honestly() {
        let r = experiment(200, 7);
        assert_eq!(r.scenarios.len(), 3);
        let ind = &r.scenarios[0];
        assert!(ind.bayes_acc > 0.75, "{ind:?}");
        let cop = &r.scenarios[1];
        assert!(cop.quantum_brier < cop.bayes_brier, "verzadiging helpt bij gekopieerde bronnen: {cop:?}");
        assert!(!r.verdict.is_empty());
    }
}
