//! Confidence calibrator (W13.06): Platt-schaling (logistische regressie op de ruwe
//! zekerheid) gefit op een dev-split, gemeten op een gescheiden test-split (ECE, Brier,
//! nauwkeurigheid). Gebruikt voor de Judge-zekerheid (gate) en weergegeven zekerheid.
// VX:W13.06

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Platt {
    pub a: f64,
    pub b: f64,
}

impl Default for Platt {
    /// Identiteit-achtig: σ(logit(p)).
    fn default() -> Self {
        Self { a: 1.0, b: 0.0 }
    }
}

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln()
}

impl Platt {
    pub fn apply(&self, p: f64) -> f64 {
        1.0 / (1.0 + (-(self.a * logit(p) + self.b)).exp())
    }

    /// Fit met gradient descent op log-loss. `data` = (ruwe zekerheid, correct 0/1).
    pub fn fit(data: &[(f64, f64)]) -> Self {
        let (mut a, mut b) = (1.0, 0.0);
        if data.len() < 5 {
            return Self { a, b };
        }
        let lr = 0.05;
        for _ in 0..3000 {
            let (mut ga, mut gb) = (0.0, 0.0);
            for (p, y) in data {
                let x = logit(*p);
                let q = 1.0 / (1.0 + (-(a * x + b)).exp());
                ga += (q - y) * x;
                gb += q - y;
            }
            let n = data.len() as f64;
            a -= lr * ga / n;
            b -= lr * gb / n;
        }
        Self { a, b }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub n: usize,
    /// Expected calibration error (10 bakken).
    pub ece: f64,
    pub brier: f64,
    pub accuracy: f64,
}

pub fn measure(data: &[(f64, f64)], f: impl Fn(f64) -> f64) -> CalibrationReport {
    if data.is_empty() {
        return CalibrationReport::default();
    }
    let mut bins = [(0.0f64, 0.0f64, 0usize); 10];
    let (mut brier, mut acc) = (0.0, 0.0);
    for (p, y) in data {
        let q = f(*p);
        let i = ((q * 10.0) as usize).min(9);
        bins[i].0 += q;
        bins[i].1 += y;
        bins[i].2 += 1;
        brier += (q - y).powi(2);
        if (q >= 0.5) == (*y >= 0.5) {
            acc += 1.0;
        }
    }
    let n = data.len() as f64;
    let ece = bins.iter().filter(|b| b.2 > 0).map(|b| (b.2 as f64 / n) * ((b.0 - b.1) / b.2 as f64).abs()).sum();
    CalibrationReport { n: data.len(), ece, brier: brier / n, accuracy: acc / n }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calibrator {
    pub platt: Platt,
    pub dev: CalibrationReport,
    pub test_raw: CalibrationReport,
    pub test_calibrated: CalibrationReport,
    pub fitted_at: String,
}

impl Calibrator {
    /// Fit op dev (deterministische split op index), meet op test.
    pub fn fit(data: &[(f64, f64)]) -> Self {
        let dev: Vec<(f64, f64)> = data.iter().enumerate().filter(|(i, _)| i % 3 != 0).map(|(_, x)| *x).collect();
        let test: Vec<(f64, f64)> = data.iter().enumerate().filter(|(i, _)| i % 3 == 0).map(|(_, x)| *x).collect();
        let platt = Platt::fit(&dev);
        Self {
            platt,
            dev: measure(&dev, |p| platt.apply(p)),
            test_raw: measure(&test, |p| p),
            test_calibrated: measure(&test, |p| platt.apply(p)),
            fitted_at: vortex_types::now().to_rfc3339(),
        }
    }

    pub fn apply(&self, p: f64) -> f64 {
        self.platt.apply(p)
    }

    pub fn load(path: &std::path::Path) -> Option<Self> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overconfident_judge_gets_calibrated() {
        // Judge zegt 0.9 maar heeft maar 60% gelijk; zegt 0.6 en heeft 30% gelijk.
        let mut data = Vec::new();
        for i in 0..300 {
            data.push((0.9, if i % 10 < 6 { 1.0 } else { 0.0 }));
            data.push((0.6, if i % 10 < 3 { 1.0 } else { 0.0 }));
        }
        let c = Calibrator::fit(&data);
        assert!(c.test_calibrated.ece < c.test_raw.ece / 2.0, "{c:?}");
        assert!((c.apply(0.9) - 0.6).abs() < 0.08, "{}", c.apply(0.9));
        assert!(c.test_calibrated.brier < c.test_raw.brier);
    }
}
