//! Datasets per layer (W11.01): voorbeelden (kenmerken, label, heuristische uitkomst, herkomst
//! van het label), versie = hash van de inhoud, decontaminatie t.o.v. de eval-set, vaste split
//! (train/test op hash van de bron), evaluatie net vs heuristiek.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::onnx::Net;
use crate::spec::{LayerKind, LayerSpec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    pub x: Vec<f32>,
    /// Klasse-index (multiclass) of 0/1 (binary).
    pub y: f32,
    /// Uitkomst van de heuristiek op het moment van de beslissing.
    pub h: Vec<f32>,
    #[serde(default = "one")]
    pub w: f32,
    /// Herkomst (run-ID, herinnering-ID, …) — bepaalt de split.
    pub src: String,
    /// "outcome" (feedback/gebruik), "lm" (bootstrap), "human".
    pub label_source: String,
    /// Tekst (alleen voor decontaminatie; niet in de dataset geschreven).
    #[serde(default, skip_serializing)]
    pub text: Option<String>,
}
fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetMeta {
    pub layer: String,
    pub version: String,
    pub n: usize,
    pub n_train: usize,
    pub n_test: usize,
    pub decontaminated: usize,
    pub features: Vec<String>,
    pub kind: LayerKind,
    pub created_at: String,
    pub train_path: PathBuf,
    pub test_path: PathBuf,
}

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn grams(s: &str, n: usize) -> HashSet<u64> {
    let w: Vec<String> =
        s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    if w.len() < n {
        return std::iter::once(fnv(w.join(" ").as_bytes())).collect();
    }
    w.windows(n).map(|g| fnv(g.join(" ").as_bytes())).collect()
}

/// Bouw de eval-index (8-grammen van alle eval-prompts).
pub fn eval_index(prompts: &[String]) -> HashSet<u64> {
    prompts.iter().flat_map(|p| grams(p, 8)).collect()
}

pub fn is_test(src: &str) -> bool {
    fnv(src.as_bytes()).is_multiple_of(5)
}

/// Schrijf train/test-JSONL. Voorbeelden met tekst die een 8-gram met de eval-set deelt vallen af.
pub fn write(
    dir: &Path,
    spec: &LayerSpec,
    examples: Vec<Example>,
    eval: &HashSet<u64>,
) -> std::io::Result<DatasetMeta> {
    let mut kept = Vec::new();
    let mut dropped = 0;
    for e in examples {
        if e.x.len() != spec.features.len() {
            continue;
        }
        if let Some(t) = &e.text
            && grams(t, 8).iter().any(|g| eval.contains(g))
        {
            dropped += 1;
            continue;
        }
        kept.push(e);
    }
    let body: String = kept.iter().map(|e| serde_json::to_string(e).unwrap_or_default()).collect::<Vec<_>>().join("\n");
    let version = format!("ds-{:012x}", fnv(body.as_bytes()) & 0xffff_ffff_ffff);
    let d = dir.join(spec.name);
    std::fs::create_dir_all(&d)?;
    let (train, test): (Vec<&Example>, Vec<&Example>) = kept.iter().partition(|e| !is_test(&e.src));
    let tp = d.join(format!("{version}.train.jsonl"));
    let vp = d.join(format!("{version}.test.jsonl"));
    std::fs::write(&tp, train.iter().map(|e| serde_json::to_string(e).unwrap_or_default() + "\n").collect::<String>())?;
    std::fs::write(&vp, test.iter().map(|e| serde_json::to_string(e).unwrap_or_default() + "\n").collect::<String>())?;
    let meta = DatasetMeta {
        layer: spec.name.into(),
        version,
        n: kept.len(),
        n_train: train.len(),
        n_test: test.len(),
        decontaminated: dropped,
        features: spec.features.iter().map(|s| s.to_string()).collect(),
        kind: spec.kind,
        created_at: vortex_types::now().to_rfc3339(),
        train_path: tp,
        test_path: vp,
    };
    std::fs::write(
        d.join(format!("{}.meta.json", meta.version)),
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    )?;
    Ok(meta)
}

pub fn read(path: &Path) -> Vec<Example> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    pub n: usize,
    /// Nauwkeurigheid van het net.
    pub net: f64,
    /// Nauwkeurigheid van de heuristiek op dezelfde voorbeelden.
    pub heuristic: f64,
}

impl EvalReport {
    /// Wint het net duidelijk (marge) van de heuristiek?
    pub fn net_wins(&self, margin: f64) -> bool {
        self.n >= 10 && self.net >= self.heuristic + margin
    }
}

fn correct(kind: LayerKind, out: &[f32], y: f32) -> bool {
    match kind {
        LayerKind::Multiclass(_) => vortex_core::neural::argmax(out) as f32 == y,
        LayerKind::Binary => (out.first().copied().unwrap_or(0.0) >= 0.5) == (y >= 0.5),
    }
}

pub fn evaluate(net: &Net, kind: LayerKind, test: &[Example]) -> EvalReport {
    let xs: Vec<Vec<f32>> = test.iter().map(|e| e.x.clone()).collect();
    let ys = net.run_batch(&xs).unwrap_or_default();
    let (mut a, mut b, mut wsum) = (0f64, 0f64, 0f64);
    for (e, y) in test.iter().zip(ys.iter()) {
        let w = f64::from(e.w);
        wsum += w;
        if correct(kind, y, e.y) {
            a += w;
        }
        if correct(kind, &e.h, e.y) {
            b += w;
        }
    }
    let d = wsum.max(1e-9);
    EvalReport { n: test.len(), net: a / d, heuristic: b / d }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decontaminates_and_splits() {
        let spec = crate::spec::get("salience").unwrap();
        let eval =
            eval_index(&["Wat is de hoofdstad van Frankrijk en hoeveel inwoners heeft die stad ongeveer".into()]);
        let mut ex: Vec<Example> = (0..50)
            .map(|i| Example {
                x: vec![0.0; 6],
                y: (i % 2) as f32,
                h: vec![0.5],
                w: 1.0,
                src: format!("run{i}"),
                label_source: "outcome".into(),
                text: Some(format!("bericht {i}")),
            })
            .collect();
        ex.push(Example {
            text: Some("Vraag: wat is de hoofdstad van Frankrijk en hoeveel inwoners heeft die stad ongeveer?".into()),
            ..ex[0].clone()
        });
        let d = tempfile::tempdir().unwrap();
        let m = write(d.path(), &spec, ex, &eval).unwrap();
        assert_eq!(m.decontaminated, 1);
        assert_eq!(m.n, 50);
        assert_eq!(m.n_train + m.n_test, 50);
        assert!(m.n_test > 0 && m.n_test < 25);
        assert_eq!(read(&m.test_path).len(), m.n_test);
        // Zelfde inhoud = zelfde versie.
        let m2 =
            write(d.path(), &spec, read(&m.train_path).into_iter().chain(read(&m.test_path)).collect(), &eval).unwrap();
        assert_eq!(m2.n, 50);
    }
}
