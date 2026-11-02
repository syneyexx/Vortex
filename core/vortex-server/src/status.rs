//! `vortex status`: GPU's (nvidia-smi), modelservers (health), kern-database.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Serialize)]
struct Gpu {
    index: u32,
    name: String,
    vram_total_mib: u64,
    vram_used_mib: u64,
}

#[derive(Debug, Serialize)]
struct Server {
    name: String,
    url: String,
    role: String,
    ready: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct Status {
    version: &'static str,
    gpus: Vec<Gpu>,
    gpu_detail: Option<String>,
    servers: Vec<Server>,
    db: String,
    active_runs: usize,
}

fn gpus() -> Result<Vec<Gpu>, String> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=index,name,memory.total,memory.used", "--format=csv,noheader,nounits"])
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
            })
        })
        .collect())
}

async fn servers(config_dir: &Path) -> Vec<Server> {
    let Ok(text) = std::fs::read_to_string(config_dir.join("models.toml")) else {
        return Vec::new();
    };
    let Ok(t) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().ok();
    let mut out = Vec::new();
    if let Some(srv) = t.get("servers").and_then(|s| s.as_table()) {
        for (name, v) in srv {
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            let role = v.get("role").and_then(|u| u.as_str()).unwrap_or_default().to_string();
            let (ready, detail) = match &client {
                Some(c) => match c.get(format!("{url}/health")).send().await {
                    Ok(r) if r.status().is_success() => (true, "ok".to_string()),
                    Ok(r) => (false, format!("HTTP {}", r.status())),
                    Err(e) => (false, if e.is_connect() { "niet bereikbaar".into() } else { e.to_string() }),
                },
                None => (false, "http-client".into()),
            };
            out.push(Server { name: name.clone(), url, role, ready, detail });
        }
    }
    out
}

pub async fn run(config_dir: &Path, data_dir: &Path, json: bool) -> anyhow::Result<()> {
    let (gpus, gpu_detail) = match gpus() {
        Ok(g) => (g, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let servers = servers(config_dir).await;
    let db_path = data_dir.join("vortex.db");
    let (db, active_runs) = if db_path.exists() {
        match vortex_core::Db::open(&db_path, vortex_core::db::CORE_MIGRATIONS) {
            Ok(db) => {
                let n = vortex_core::RunStore::new(vortex_core::EventLog::new(std::sync::Arc::new(db)))
                    .active()
                    .map(|v| v.len())
                    .unwrap_or(0);
                ("ok".to_string(), n)
            }
            Err(e) => (format!("fout: {e}"), 0),
        }
    } else {
        ("nog niet aangemaakt (start `vortex serve`)".to_string(), 0)
    };
    let st = Status { version: env!("CARGO_PKG_VERSION"), gpus, gpu_detail, servers, db, active_runs };
    if json {
        println!("{}", serde_json::to_string_pretty(&st)?);
        return Ok(());
    }
    println!("Vortex {}", st.version);
    println!("\nGPU's:");
    if st.gpus.is_empty() {
        println!("  geen ({})", st.gpu_detail.as_deref().unwrap_or("onbekend"));
    }
    for g in &st.gpus {
        println!("  [{}] {:<28} {:>6} / {:>6} MiB", g.index, g.name, g.vram_used_mib, g.vram_total_mib);
    }
    println!("\nModelservers:");
    if st.servers.is_empty() {
        println!("  geen geconfigureerd (config/models.toml → [servers.*])");
    }
    for s in &st.servers {
        println!("  {:<6} {:<5} {:<28} {}", s.name, if s.ready { "OK" } else { "--" }, s.url, s.detail);
    }
    println!("\nKern-database: {} · actieve runs: {}", st.db, st.active_runs);
    Ok(())
}
