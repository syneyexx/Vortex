//! GPU-bewaker (W15.01): idle-detectie komt van de sampler (OS-invoer + nvidia-smi → Dirigent).
//! Hier: beheerde llama-servers (`managed = true` in models.toml) worden ontladen zodra een
//! ander programma (bv. een game) de GPU gebruikt en Vortex niets aan het doen is — VRAM direct
//! vrij — en daarna weer geladen. Een expliciet verzoek van de gebruiker laadt meteen.
// VX:W15.01

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_core::{EventLog, Kernel};
use vortex_models::{LaunchConfig, ModelEntry, ModelServer, Phase, Registry};
use vortex_types::{Event, EventKind, TraceId};

#[derive(Debug, Clone)]
struct Managed {
    srv: ModelServer,
    home: String,
    entry: ModelEntry,
}

#[derive(Debug)]
pub struct GpuGuard {
    log: EventLog,
    servers: Mutex<HashMap<String, Managed>>,
    released: AtomicBool,
    clear_since: Mutex<Option<Instant>>,
    pub restore_after: Mutex<Duration>,
    pub enabled: AtomicBool,
}

impl GpuGuard {
    pub fn new(log: EventLog) -> Self {
        Self {
            log,
            servers: Mutex::new(HashMap::new()),
            released: AtomicBool::new(false),
            clear_since: Mutex::new(None),
            restore_after: Mutex::new(Duration::from_secs(60)),
            enabled: AtomicBool::new(true),
        }
    }

    /// Beheerde servers uit de registry opzetten (programma = `paths.llama_server`).
    pub fn configure(&self, kernel: &Kernel, data_dir: &std::path::Path) {
        let cfg = kernel.config().current();
        let Ok(reg) = Registry::from_config(&cfg) else { return };
        let program = cfg.get::<String>("paths.llama_server").ok().flatten().unwrap_or_else(|| "llama-server".into());
        let auto = cfg.values.get("autonomy").cloned().unwrap_or(toml::Value::Table(Default::default()));
        if let Some(s) = auto.get("gpu").and_then(|g| g.get("restore_after_secs")).and_then(|v| v.as_integer()) {
            *self.restore_after.lock() = Duration::from_secs(s.max(0) as u64);
        }
        if let Some(b) = auto.get("gpu").and_then(|g| g.get("release_on_external_load")).and_then(|v| v.as_bool()) {
            self.enabled.store(b, Ordering::SeqCst);
        }
        let mut g = self.servers.lock();
        for (name, s) in reg.servers.iter().filter(|(_, s)| s.managed) {
            if g.contains_key(name) {
                continue;
            }
            let Some(home) = s.model.clone() else { continue };
            let Some(entry) = reg.models.get(&home).cloned() else { continue };
            let port = s.url.rsplit(':').next().and_then(|p| p.trim_end_matches('/').parse().ok()).unwrap_or(8080);
            let lc = LaunchConfig {
                program: program.clone(),
                program_args: s.launch_args.clone(),
                host: "127.0.0.1".into(),
                port,
                gpu: s.gpu,
                log_dir: data_dir.join("logs"),
                health_timeout: Duration::from_secs(180),
                drain_timeout: Duration::from_secs(20),
                slot_save_dir: Some(data_dir.join("slots")),
            };
            g.insert(
                name.clone(),
                Managed { srv: ModelServer::new(name, lc, kernel.supervisor().clone(), self.log.clone()), home, entry },
            );
        }
    }

    pub fn managed(&self) -> Vec<String> {
        self.servers.lock().keys().cloned().collect()
    }

    pub fn is_released(&self) -> bool {
        self.released.load(Ordering::SeqCst)
    }

    /// Alle beheerde servers met hun thuismodel laden (bij start en na vrijgeven).
    pub async fn load_all(&self) -> Vec<Value> {
        let all: Vec<(String, Managed)> = self.servers.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut out = Vec::new();
        for (name, m) in all {
            if m.srv.phase().await == Phase::Ready {
                continue;
            }
            let r = m.srv.load(&m.home, &m.entry).await;
            out.push(json!({"server": name, "ok": r.is_ok(), "error": r.err().map(|e| e.to_string())}));
        }
        out
    }

    /// Alle beheerde servers ontladen (bv. voor beeldgeneratie). Geeft het aantal.
    pub async fn unload_all(&self) -> usize {
        let all: Vec<Managed> = self.servers.lock().values().cloned().collect();
        let mut n = 0;
        for m in all {
            if m.srv.phase().await == Phase::Ready && m.srv.unload().await.is_ok() {
                n += 1;
            }
        }
        n
    }

    /// Zorg dat de server van `model` draait (expliciet verzoek: wint van de game).
    pub async fn ensure(&self, reg: &Registry, model: &str) {
        let Some(server) = reg.models.get(model).map(|e| e.gpu.clone()) else { return };
        let m = self.servers.lock().get(&server).cloned();
        if let Some(m) = m
            && m.srv.phase().await != Phase::Ready
        {
            let entry = reg.models.get(model).cloned().unwrap_or(m.entry.clone());
            let _ = m.srv.load(model, &entry).await;
        }
    }

    /// Eén stap: `external` = ander programma gebruikt de GPU (Dirigent-conditie).
    /// Geeft "released", "restored" of niets.
    pub async fn step(&self, external: bool) -> Option<&'static str> {
        if !self.enabled.load(Ordering::SeqCst) || self.servers.lock().is_empty() {
            return None;
        }
        let all: Vec<(String, Managed)> = self.servers.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        if external {
            *self.clear_since.lock() = None;
            if self.is_released() || all.iter().any(|(_, m)| m.srv.inflight() > 0) {
                return None; // al vrij, of Vortex is zelf nog bezig voor de gebruiker
            }
            let mut freed = Vec::new();
            for (name, m) in &all {
                if m.srv.phase().await == Phase::Ready && m.srv.unload().await.is_ok() {
                    freed.push(name.clone());
                }
            }
            self.released.store(true, Ordering::SeqCst);
            let _ = self.log.append(Event::new(
                EventKind::GpuReleased,
                TraceId::new(),
                json!({"servers": freed, "reason": "ander programma gebruikt de GPU"}),
            ));
            return Some("released");
        }
        if !self.is_released() {
            return None;
        }
        let since = *self.clear_since.lock().get_or_insert_with(Instant::now);
        if since.elapsed() < *self.restore_after.lock() {
            return None;
        }
        let r = self.load_all().await;
        self.released.store(false, Ordering::SeqCst);
        *self.clear_since.lock() = None;
        let _ = self.log.append(Event::new(EventKind::GpuRestored, TraceId::new(), json!({"servers": r})));
        Some("restored")
    }

    pub async fn status(&self) -> Value {
        let all: Vec<(String, Managed)> = self.servers.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut s = Vec::new();
        for (name, m) in all {
            s.push(json!({"server": name, "home": m.home, "phase": format!("{:?}", m.srv.phase().await), "model": m.srv.current_model().await, "inflight": m.srv.inflight()}));
        }
        json!({"enabled": self.enabled.load(Ordering::SeqCst), "released": self.is_released(), "restore_after_secs": self.restore_after.lock().as_secs(), "servers": s})
    }
}

/// Bewaker starten: beheerde servers laden en elke 2 s de condities volgen.
pub fn spawn(guard: Arc<GpuGuard>, kernel: Kernel, data_dir: PathBuf, stop: CancellationToken) {
    guard.configure(&kernel, &data_dir);
    if guard.managed().is_empty() {
        return;
    }
    tokio::spawn(async move {
        let _ = guard.load_all().await;
        loop {
            tokio::select! {
                _ = stop.cancelled() => {
                    let all: Vec<Managed> = guard.servers.lock().values().cloned().collect();
                    for m in all {
                        let _ = m.srv.unload().await;
                    }
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
            let ext = kernel.dirigent().conditions().external_gpu_load;
            guard.step(ext).await;
        }
    });
}
