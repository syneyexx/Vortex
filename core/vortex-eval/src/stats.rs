//! Gepaarde vergelijking (A/B, W17.05): per taak score A vs B, gemiddeld verschil met een
//! bootstrap-95%-interval (vaste seed = herhaalbaar) en een tekentoets op winst/verlies.
// VX:W17.05

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Paired {
    pub n: usize,
    pub mean_a: f64,
    pub mean_b: f64,
    /// Gemiddelde van (B − A).
    pub diff: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub b_wins: usize,
    pub a_wins: usize,
    pub ties: usize,
    /// Tweezijdige tekentoets (binomiaal, ties weg).
    pub p_sign: f64,
    /// "B" / "A" / "geen verschil" (CI sluit 0 uit én p < 0,05).
    pub verdict: String,
}

fn binom_two_sided(k: usize, n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    // P(X <= min(k, n-k)) * 2 bij p = 0,5, in log-ruimte.
    let m = k.min(n - k);
    let ln_fact = |x: usize| (1..=x).map(|i| (i as f64).ln()).sum::<f64>();
    let lc = |i: usize| ln_fact(n) - ln_fact(i) - ln_fact(n - i) - n as f64 * std::f64::consts::LN_2;
    ((0..=m).map(|i| lc(i).exp()).sum::<f64>() * 2.0).min(1.0)
}

/// `pairs` = (taak, score A, score B).
pub fn paired(pairs: &[(String, f64, f64)]) -> Paired {
    let n = pairs.len();
    if n == 0 {
        return Paired { verdict: "geen data".into(), p_sign: 1.0, ..Default::default() };
    }
    let d: Vec<f64> = pairs.iter().map(|p| p.2 - p.1).collect();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let diff = mean(&d);
    // Bootstrap (xorshift, vaste seed).
    let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut boots: Vec<f64> = (0..2000)
        .map(|_| {
            (0..n)
                .map(|_| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    d[(s % n as u64) as usize]
                })
                .sum::<f64>()
                / n as f64
        })
        .collect();
    boots.sort_by(f64::total_cmp);
    let (lo, hi) = (boots[50], boots[1949]);
    let b_wins = d.iter().filter(|x| **x > 1e-9).count();
    let a_wins = d.iter().filter(|x| **x < -1e-9).count();
    let p = binom_two_sided(b_wins, b_wins + a_wins);
    let verdict = if lo > 0.0 && p < 0.05 {
        "B"
    } else if hi < 0.0 && p < 0.05 {
        "A"
    } else {
        "geen verschil"
    };
    Paired {
        n,
        mean_a: pairs.iter().map(|p| p.1).sum::<f64>() / n as f64,
        mean_b: pairs.iter().map(|p| p.2).sum::<f64>() / n as f64,
        diff,
        ci_low: lo,
        ci_high: hi,
        b_wins,
        a_wins,
        ties: n - b_wins - a_wins,
        p_sign: p,
        verdict: verdict.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_real_difference_and_not_noise() {
        let better: Vec<(String, f64, f64)> =
            (0..30).map(|i| (format!("t{i}"), 0.0, if i % 10 == 0 { 0.0 } else { 1.0 })).collect();
        let r = paired(&better);
        assert_eq!(r.verdict, "B");
        assert!(r.ci_low > 0.0 && r.p_sign < 0.001);
        let noise: Vec<(String, f64, f64)> =
            (0..30).map(|i| (format!("t{i}"), (i % 2) as f64, ((i + 1) % 2) as f64)).collect();
        assert_eq!(paired(&noise).verdict, "geen verschil");
        assert!((binom_two_sided(0, 10) - 0.001953125).abs() < 1e-9);
    }
}
