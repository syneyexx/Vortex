//! W11-gate in Rust: een net dat de echte regel kent wint van de heuristiek → promotie
//! (hot-swap: volgende beslissing komt van het net), schaduwlogging, en een actieve versie die
//! op nieuwe data verliest wordt automatisch teruggedraaid naar de heuristiek.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use vortex_core::db::CORE_MIGRATIONS;
use vortex_core::{Db, EventLog, LayerScorer};
use vortex_neural::dataset::{Example, evaluate};
use vortex_neural::onnx::mlp_to_onnx;
use vortex_neural::registry::VersionInfo;
use vortex_neural::{LayerKind, LayerRegistry, Mode, Net};
use vortex_types::EventKind;

/// Salience-achtige taak: belangrijk ⇔ x0 + x2 > 1. Heuristiek kijkt alleen naar x0 > 0.5.
fn data(n: usize, seed: u64, flip: bool) -> Vec<Example> {
    let mut s = seed;
    let mut rnd = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as f32) / (1u64 << 31) as f32
    };
    (0..n)
        .map(|i| {
            let x: Vec<f32> = (0..6).map(|_| rnd()).collect();
            let truth = x[0] + x[2] > 1.0;
            let y = if flip { !truth } else { truth };
            Example {
                h: vec![if x[0] > 0.5 { 0.9 } else { 0.1 }],
                y: if y { 1.0 } else { 0.0 },
                x,
                w: 1.0,
                src: format!("r{seed}-{i}"),
                label_source: "outcome".into(),
                text: None,
            }
        })
        .collect()
}

/// Net dat x0 + x2 - 1 leert (steile sigmoid).
fn rule_net(dir: &std::path::Path) -> std::path::PathBuf {
    let l1 = (vec![vec![10.0, 0.0, 10.0, 0.0, 0.0, 0.0]], vec![-10.0]);
    let bytes = mlp_to_onnx(&[l1], "", "sigmoid", None, &[("layer", "salience")]);
    let p = dir.join("rule.onnx");
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn net_beats_heuristic_promotes_then_rolls_back() {
    let d = tempfile::tempdir().unwrap();
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap()));
    let reg = LayerRegistry::open(d.path().join("layers"), Some(log.clone()));
    let x = vec![0.2, 0.0, 0.95, 0.0, 0.0, 0.0]; // waarheid: belangrijk; heuristiek: niet
    // Zonder versie: heuristiek.
    assert_eq!(reg.decide("salience", &x, vec![0.1], "run1"), vec![0.1]);

    // Installeren = schaduw: heuristiek telt nog, net draait mee en wordt gelogd.
    let v = reg
        .install("salience", &rule_net(d.path()), VersionInfo { version: "v1".into(), ..Default::default() })
        .unwrap();
    assert_eq!(reg.manifest("salience").mode, Mode::Shadow);
    assert_eq!(reg.decide("salience", &x, vec![0.1], "run2"), vec![0.1]);
    reg.flush();
    let ev = log.read_from(0, 100).unwrap();
    let sh = ev.iter().find(|e| e.kind == EventKind::NeuralShadow).unwrap();
    assert!(sh.payload["net"][0].as_f64().unwrap() > 0.5, "net ziet het wel: {}", sh.payload);
    assert!(ev.iter().any(|e| e.kind == EventKind::NeuralExample && e.payload["ref"] == "run2"), "feature logging");

    // Eval: net wint duidelijk → promotie.
    let test = data(300, 7, false);
    let net = Net::load(&d.path().join("layers/salience").join(format!("{v}.onnx"))).unwrap();
    let se = evaluate(&net, LayerKind::Binary, &test);
    assert!(se.net > 0.97 && se.heuristic < 0.85, "{se:?}");
    let msg = reg.auto_decide("salience", Some(&se), None, 0.02).unwrap();
    assert!(msg.contains("gepromoveerd"), "{msg}");
    // Hot-swap: zelfde registry, volgende beslissing komt van het net.
    let out = reg.decide("salience", &x, vec![0.1], "run3");
    assert!(out[0] > 0.5, "{out:?}");
    assert_eq!(reg.status().iter().find(|s| s.layer == "salience").unwrap().manifest.active.as_deref(), Some("v1"));

    // Wereld verandert (labels omgekeerd): actieve versie verliest van heuristiek → rollback.
    let drift = data(300, 9, true);
    let ae = evaluate(&net, LayerKind::Binary, &drift);
    assert!(ae.net < ae.heuristic, "{ae:?}");
    let msg = reg.auto_decide("salience", None, Some(&ae), 0.02).unwrap();
    assert!(msg.contains("teruggedraaid naar heuristiek"), "{msg}");
    assert_eq!(reg.decide("salience", &x, vec![0.1], "run4"), vec![0.1], "terug op de heuristiek");
    reg.flush();
    assert!(log.read_from(0, 500).unwrap().iter().any(|e| e.kind == EventKind::NeuralRolledBack));

    // Manifest overleeft herstart (nieuwe registry op dezelfde map).
    let reg2 = LayerRegistry::open(d.path().join("layers"), None);
    assert_eq!(reg2.manifest("salience").active, None);
    assert_eq!(reg2.manifest("salience").previous, Vec::<String>::new());
}

#[test]
fn install_rejects_wrong_input_dim() {
    let d = tempfile::tempdir().unwrap();
    let reg = LayerRegistry::open(d.path(), None);
    let bytes = mlp_to_onnx(&[(vec![vec![1.0, 1.0]], vec![0.0])], "", "sigmoid", None, &[]);
    let p = d.path().join("x.onnx");
    std::fs::write(&p, bytes).unwrap();
    assert!(reg.install("salience", &p, VersionInfo::default()).unwrap_err().contains("kenmerken"));
}
