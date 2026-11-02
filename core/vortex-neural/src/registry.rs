//! Versies per layer, schaduwmodus, promotie/rollback op eval en hot-swap (W11.04, W11.07).
//! Layout: `<dir>/<layer>/<versie>.onnx` + `manifest.json` (actief, schaduw, vorige, scores).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use vortex_core::{EventLog, LayerScorer};
use vortex_types::{Event, EventKind, TraceId};

use crate::dataset::EvalReport;
use crate::onnx::Net;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Alleen heuristiek.
    #[default]
    Off,
    /// Net draait mee en wordt gelogd; heuristiek telt.
    Shadow,
    /// Net beslist.
    Active,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub created_at: String,
    pub dataset: String,
    pub eval: Option<EvalReport>,
    #[serde(default)]
    pub train_metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub mode: Mode,
    pub active: Option<String>,
    pub shadow: Option<String>,
    pub previous: Vec<String>,
    pub versions: Vec<VersionInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerStatus {
    pub layer: String,
    pub manifest: Manifest,
    /// Schaduwvergelijkingen: (aantal, eens met heuristiek).
    pub shadow_n: u64,
    pub shadow_agree: u64,
    pub decisions: u64,
}

#[derive(Debug, Default)]
struct Loaded {
    manifest: Manifest,
    active: Option<Arc<Net>>,
    shadow: Option<Arc<Net>>,
}

#[derive(Debug, Default)]
struct Counters {
    decisions: AtomicU64,
    shadow_n: AtomicU64,
    shadow_agree: AtomicU64,
}

/// Events gaan via een eigen schrijfthread naar het log: `decide` blokkeert nooit op de database
/// (en kan dus veilig binnen een transactie van een aanroeper gebruikt worden).
#[derive(Debug)]
struct Emitter {
    tx: parking_lot::Mutex<std::sync::mpsc::Sender<Event>>,
    pending: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct LayerRegistry {
    dir: PathBuf,
    log: Option<Emitter>,
    layers: RwLock<HashMap<String, Loaded>>,
    counters: RwLock<HashMap<String, Arc<Counters>>>,
    /// Elk N-de voorbeeld loggen (feature logging); 1 = alles.
    pub log_every: u64,
}

fn agree(a: &[f32], b: &[f32]) -> bool {
    if a.len() > 1 {
        vortex_core::neural::argmax(a) == vortex_core::neural::argmax(b)
    } else {
        (a.first() >= Some(&0.5)) == (b.first() >= Some(&0.5))
    }
}

impl LayerRegistry {
    pub fn open(dir: impl Into<PathBuf>, log: Option<EventLog>) -> Arc<Self> {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(&dir);
        let log = log.map(|l| {
            let (tx, rx) = std::sync::mpsc::channel::<Event>();
            let pending = Arc::new(AtomicU64::new(0));
            let p2 = pending.clone();
            std::thread::Builder::new()
                .name("vortex-neural-log".into())
                .spawn(move || {
                    while let Ok(e) = rx.recv() {
                        let _ = l.append(e);
                        p2.fetch_sub(1, Ordering::SeqCst);
                    }
                })
                .ok();
            Emitter { tx: parking_lot::Mutex::new(tx), pending }
        });
        let r = Arc::new(Self {
            dir,
            log,
            layers: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
            log_every: 1,
        });
        for s in crate::spec::specs() {
            r.reload(s.name);
        }
        r
    }

    fn mpath(&self, layer: &str) -> PathBuf {
        self.dir.join(layer).join("manifest.json")
    }

    fn net_path(&self, layer: &str, version: &str) -> PathBuf {
        self.dir.join(layer).join(format!("{version}.onnx"))
    }

    /// Manifest + netten (opnieuw) laden — hot-swap: lopende beslissingen houden hun `Arc<Net>`.
    pub fn reload(&self, layer: &str) {
        let manifest: Manifest = std::fs::read_to_string(self.mpath(layer))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let load = |v: &Option<String>| v.as_ref().and_then(|v| Net::load(&self.net_path(layer, v)).ok()).map(Arc::new);
        let l = Loaded { active: load(&manifest.active), shadow: load(&manifest.shadow), manifest };
        self.layers.write().insert(layer.to_string(), l);
    }

    fn save(&self, layer: &str, m: &Manifest) -> Result<(), String> {
        let p = self.mpath(layer);
        std::fs::create_dir_all(p.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(m).unwrap_or_default()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        self.reload(layer);
        Ok(())
    }

    pub fn manifest(&self, layer: &str) -> Manifest {
        self.layers.read().get(layer).map(|l| l.manifest.clone()).unwrap_or_default()
    }

    fn emit(&self, kind: EventKind, payload: serde_json::Value) {
        if let Some(l) = &self.log {
            l.pending.fetch_add(1, Ordering::SeqCst);
            if l.tx.lock().send(Event::new(kind, TraceId::new(), payload)).is_err() {
                l.pending.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Wacht tot alle events geschreven zijn (tests, afsluiten).
    pub fn flush(&self) {
        if let Some(l) = &self.log {
            for _ in 0..2000 {
                if l.pending.load(Ordering::SeqCst) == 0 {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    /// Nieuwe versie installeren (als schaduw). Het ONNX-bestand wordt gekopieerd en gevalideerd.
    pub fn install(&self, layer: &str, onnx: &Path, info: VersionInfo) -> Result<String, String> {
        let spec = crate::spec::get(layer).ok_or_else(|| format!("onbekende layer {layer}"))?;
        let net = Net::load(onnx)?;
        if net.input_dim != spec.features.len() {
            return Err(format!(
                "net verwacht {} kenmerken, layer {layer} heeft er {}",
                net.input_dim,
                spec.features.len()
            ));
        }
        let mut m = self.manifest(layer);
        let version = if info.version.is_empty() { format!("v{}", m.versions.len() + 1) } else { info.version.clone() };
        let dst = self.net_path(layer, &version);
        std::fs::create_dir_all(dst.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
        std::fs::copy(onnx, &dst).map_err(|e| e.to_string())?;
        m.versions.retain(|v| v.version != version);
        m.versions.push(VersionInfo { version: version.clone(), ..info });
        m.shadow = Some(version.clone());
        if m.mode == Mode::Off {
            m.mode = Mode::Shadow;
        }
        self.save(layer, &m)?;
        self.emit(EventKind::NeuralTrained, serde_json::json!({"layer": layer, "version": version}));
        Ok(version)
    }

    /// Schaduwversie → actief (vorige actieve blijft bewaard voor rollback).
    pub fn promote(&self, layer: &str) -> Result<String, String> {
        let mut m = self.manifest(layer);
        let v = m.shadow.take().ok_or("geen schaduwversie")?;
        if let Some(a) = m.active.take() {
            m.previous.push(a);
        }
        m.active = Some(v.clone());
        m.mode = Mode::Active;
        self.save(layer, &m)?;
        self.emit(EventKind::NeuralPromoted, serde_json::json!({"layer": layer, "version": v}));
        Ok(v)
    }

    /// Terug naar de vorige versie; zonder vorige → heuristiek (mode off).
    pub fn rollback(&self, layer: &str, reason: &str) -> Result<Option<String>, String> {
        let mut m = self.manifest(layer);
        let from = m.active.take();
        m.active = m.previous.pop();
        if m.active.is_none() {
            m.mode = if m.shadow.is_some() { Mode::Shadow } else { Mode::Off };
        }
        self.save(layer, &m)?;
        self.emit(
            EventKind::NeuralRolledBack,
            serde_json::json!({"layer": layer, "from": from, "to": m.active, "reason": reason}),
        );
        Ok(m.active.clone())
    }

    pub fn set_mode(&self, layer: &str, mode: Mode) -> Result<(), String> {
        let mut m = self.manifest(layer);
        if mode == Mode::Active && m.active.is_none() {
            return Err("geen actieve versie (eerst promoveren)".into());
        }
        m.mode = mode;
        self.save(layer, &m)
    }

    /// Eval-resultaat bij een versie opslaan.
    pub fn record_eval(&self, layer: &str, version: &str, r: EvalReport) -> Result<(), String> {
        let mut m = self.manifest(layer);
        if let Some(v) = m.versions.iter_mut().find(|v| v.version == version) {
            v.eval = Some(r);
        }
        self.save(layer, &m)
    }

    /// Automatische promotie/rollback (W11.07): schaduw wint duidelijk van heuristiek én van de
    /// actieve versie → promoveren; actieve versie verliest van de heuristiek → terugdraaien.
    pub fn auto_decide(
        &self,
        layer: &str,
        shadow_eval: Option<&EvalReport>,
        active_eval: Option<&EvalReport>,
        margin: f64,
    ) -> Result<String, String> {
        if let Some(a) = active_eval
            && a.n >= 10
            && a.net + 1e-9 < a.heuristic
        {
            let to = self.rollback(layer, &format!("actief {:.3} < heuristiek {:.3}", a.net, a.heuristic))?;
            return Ok(format!("teruggedraaid naar {}", to.unwrap_or_else(|| "heuristiek".into())));
        }
        if let Some(s) = shadow_eval
            && s.net_wins(margin)
            && active_eval.is_none_or(|a| s.net > a.net)
        {
            let v = self.promote(layer)?;
            return Ok(format!("{v} gepromoveerd ({:.3} vs heuristiek {:.3})", s.net, s.heuristic));
        }
        Ok("geen wijziging".into())
    }

    pub fn status(&self) -> Vec<LayerStatus> {
        let c = self.counters.read();
        let l = self.layers.read();
        crate::spec::specs()
            .into_iter()
            .map(|s| {
                let k = c.get(s.name).cloned().unwrap_or_default();
                LayerStatus {
                    layer: s.name.into(),
                    manifest: l.get(s.name).map(|x| x.manifest.clone()).unwrap_or_default(),
                    shadow_n: k.shadow_n.load(Ordering::Relaxed),
                    shadow_agree: k.shadow_agree.load(Ordering::Relaxed),
                    decisions: k.decisions.load(Ordering::Relaxed),
                }
            })
            .collect()
    }

    fn counters(&self, layer: &str) -> Arc<Counters> {
        if let Some(c) = self.counters.read().get(layer) {
            return c.clone();
        }
        self.counters.write().entry(layer.to_string()).or_default().clone()
    }
}

impl LayerScorer for LayerRegistry {
    fn decide(&self, layer: &str, features: &[f32], heuristic: Vec<f32>, reference: &str) -> Vec<f32> {
        let k = self.counters(layer);
        let n = k.decisions.fetch_add(1, Ordering::Relaxed);
        let (mode, active, shadow, av, sv) = {
            let g = self.layers.read();
            match g.get(layer) {
                Some(l) => (
                    l.manifest.mode,
                    l.active.clone(),
                    l.shadow.clone(),
                    l.manifest.active.clone(),
                    l.manifest.shadow.clone(),
                ),
                None => (Mode::Off, None, None, None, None),
            }
        };
        let out = match (mode, &active) {
            (Mode::Active, Some(net)) => net.run(features).ok(),
            _ => None,
        };
        // Schaduw: nieuwste schaduwversie (of actieve in schaduwmodus) draait mee.
        let shadow_out = match (mode, &shadow, &active) {
            (Mode::Shadow | Mode::Active, Some(s), _) => s.run(features).ok().map(|o| (o, sv.clone())),
            (Mode::Shadow, None, Some(a)) => a.run(features).ok().map(|o| (o, av.clone())),
            _ => None,
        };
        if let Some((o, v)) = &shadow_out {
            k.shadow_n.fetch_add(1, Ordering::Relaxed);
            if agree(o, &heuristic) {
                k.shadow_agree.fetch_add(1, Ordering::Relaxed);
            }
            if n.is_multiple_of(self.log_every.max(1)) {
                self.emit(EventKind::NeuralShadow, serde_json::json!({"layer": layer, "version": v, "net": o, "heuristic": heuristic, "ref": reference}));
            }
        }
        // Feature logging (W11.01): elk voorbeeld met heuristiek en referentie.
        if n.is_multiple_of(self.log_every.max(1)) {
            self.emit(
                EventKind::NeuralExample,
                serde_json::json!({"layer": layer, "x": features, "h": heuristic, "ref": reference, "used": if out.is_some() { "net" } else { "heuristic" }, "version": if out.is_some() { av } else { None }}),
            );
        }
        out.unwrap_or(heuristic)
    }
}
