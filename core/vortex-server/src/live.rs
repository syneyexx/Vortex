//! Live-toestand voor de statusbalk: GPU's (nvidia-smi), modelservers (/health), tok/s-reeks.
// VX:W03.11

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use vortex_core::Kernel;
use vortex_types::EventKind;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    pub vram_total_mib: u64,
    pub vram_used_mib: u64,
    pub util_pct: Option<u32>,
    pub temp_c: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ServerHealth {
    pub name: String,
    pub url: String,
    pub role: String,
    pub model: Option<String>,
    pub ready: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Subsystem {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LiveState {
    pub gpus: Vec<Gpu>,
    pub gpu_detail: Option<String>,
    pub servers: Vec<ServerHealth>,
    /// Laatste decode-snelheden (tok/s), oud → nieuw.
    pub tok_s: VecDeque<f64>,
}

pub fn read_gpus() -> Result<Vec<Gpu>, String> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,memory.used,utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .map_err(|e| format!("nvidia-smi niet beschikbaar: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<_> = l.split(',').map(str::trim).collect();
            Some(Gpu {
                index: f.first()?.parse().ok()?,
                name: f.get(1)?.to_string(),
                vram_total_mib: f.get(2)?.parse().ok()?,
                vram_used_mib: f.get(3)?.parse().ok()?,
                util_pct: f.get(4).and_then(|v| v.parse().ok()),
                temp_c: f.get(5).and_then(|v| v.parse().ok()),
            })
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct Live(Arc<Mutex<LiveState>>);

impl Live {
    pub fn snapshot(&self) -> LiveState {
        self.0.lock().clone()
    }

    /// Achtergrondpoller (GPU elke 2 s, servers elke 5 s, tok/s uit events).
    pub fn spawn(&self, kernel: Kernel, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().unwrap_or_default();
            let mut rx = kernel.events().subscribe();
            let mut tick: u64 = 0;
            loop {
                let gpus = tokio::task::spawn_blocking(read_gpus).await.unwrap_or_else(|e| Err(e.to_string()));
                {
                    let mut s = me.0.lock();
                    match gpus {
                        Ok(g) => {
                            s.gpus = g;
                            s.gpu_detail = None;
                        }
                        Err(e) => {
                            s.gpus.clear();
                            s.gpu_detail = Some(e);
                        }
                    }
                }
                if tick.is_multiple_of(5) {
                    let servers = check_servers(&kernel, &client).await;
                    me.0.lock().servers = servers;
                }
                tick += 1;
                let deadline = tokio::time::sleep(Duration::from_secs(2));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => return,
                        _ = &mut deadline => break,
                        ev = rx.recv() => {
                            if let Ok(e) = ev
                                && e.kind == EventKind::InferenceMeasured
                                && let Some(t) = e.payload.get("decode_tps").and_then(|v| v.as_f64())
                            {
                                let mut s = me.0.lock();
                                s.tok_s.push_back(t);
                                while s.tok_s.len() > 60 { s.tok_s.pop_front(); }
                            }
                        }
                    }
                }
            }
        })
    }
}

async fn check_servers(kernel: &Kernel, client: &reqwest::Client) -> Vec<ServerHealth> {
    let Ok(reg) = vortex_models::Registry::from_config(&kernel.config().current()) else { return Vec::new() };
    let mut out = Vec::new();
    for (name, s) in &reg.servers {
        let (ready, detail) = match client.get(format!("{}/health", s.url)).send().await {
            Ok(r) if r.status().is_success() => (true, "gereed".to_string()),
            Ok(r) => (false, format!("HTTP {}", r.status())),
            Err(e) => (false, if e.is_connect() { "niet bereikbaar".into() } else { e.to_string() }),
        };
        out.push(ServerHealth {
            name: name.clone(),
            url: s.url.clone(),
            role: s.role.clone(),
            model: s.model.clone(),
            ready,
            detail,
        });
    }
    out
}
