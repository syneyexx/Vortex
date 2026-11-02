//! Harnassen voor de grote tests (🔒 W06.11 / W06.12): draaien alleen op commando van de
//! gebruiker ("GROTE TESTEN NU"). `--quick` = korte controle dat het harnas werkt.
// VX:W06.12

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::{Value, json};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

struct Api {
    c: reqwest::Client,
    base: String,
}

impl Api {
    async fn get(&self, p: &str) -> anyhow::Result<Value> {
        Ok(self.c.get(format!("{}{p}", self.base)).send().await?.json().await?)
    }
    async fn post(&self, p: &str, b: Value) -> anyhow::Result<Value> {
        Ok(self.c.post(format!("{}{p}", self.base)).json(&b).send().await?.json().await?)
    }
}

fn pct(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(((v.len() as f64 * p).ceil() as usize).saturating_sub(1)).copied().unwrap_or(f64::NAN)
}

/// `vortex perf`: prestatiedoelen meten (retrieval p95, stop-latency, TTFT, idle-CPU).
pub async fn perf(base: &str, quick: bool) -> anyhow::Result<bool> {
    let api = Api { c: reqwest::Client::new(), base: base.into() };
    let st = api.get("/api/status").await.context("kern niet bereikbaar")?;
    let pid = st["pid"].as_u64().unwrap_or(0) as u32;
    let n = if quick { 10 } else { 200 };
    let mut ok = true;
    let mut row = |name: &str, val: f64, target: f64, unit: &str| {
        let pass = val <= target;
        ok &= pass;
        println!("{:<26} {:>9.1} {unit:<4} doel ≤ {target} {unit}  {}", name, val, if pass { "OK" } else { "ROOD" });
    };
    // Retrieval p95
    let mut lat = Vec::new();
    for i in 0..n {
        let t = Instant::now();
        api.post("/api/memory/search", json!({"text": format!("testvraag {i} over modellen en geheugen")})).await?;
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    row("retrieval p95", pct(lat, 0.95), 150.0, "ms");
    // Stop-latency + TTFT (via een echte chat)
    let mut stops = Vec::new();
    let mut ttfts = Vec::new();
    for _ in 0..if quick { 1 } else { 10 } {
        let sid = api.post("/api/sessions", json!({})).await?["id"].as_str().unwrap_or_default().to_string();
        let t0 = Instant::now();
        let r = api
            .post(
                &format!("/api/sessions/{sid}/messages"),
                json!({"text": "Schrijf een lang verhaal van 2000 woorden over de zee.", "mode": "quick"}),
            )
            .await?;
        let run = r["run_id"].as_str().unwrap_or_default().to_string();
        loop {
            let s = api.get(&format!("/api/sessions/{sid}")).await?;
            if !s["messages"][1]["content"].as_str().unwrap_or("").is_empty()
                || s["messages"][1]["status"] != "streaming"
            {
                ttfts.push(t0.elapsed().as_secs_f64() * 1000.0);
                break;
            }
            anyhow::ensure!(t0.elapsed() < Duration::from_secs(120), "geen eerste token binnen 120 s");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let ts = Instant::now();
        api.post(&format!("/api/runs/{run}/stop"), json!({})).await?;
        loop {
            let s = api.get(&format!("/api/sessions/{sid}")).await?;
            if s["messages"][1]["status"] != "streaming" {
                stops.push(ts.elapsed().as_secs_f64() * 1000.0);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    row("stop → gestopt", pct(stops, 0.95), 500.0, "ms");
    println!(
        "{:<26} {:>9.1} ms   (doel wordt vastgezet na meting in W00)",
        "eerste token (p95, incl. GUI-pad)",
        pct(ttfts, 0.95)
    );
    // Idle CPU van de kern
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::new().with_cpu());
    let mut cpu = Vec::new();
    for _ in 0..if quick { 3 } else { 30 } {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::new().with_cpu());
        if let Some(pr) = sys.process(p) {
            cpu.push(pr.cpu_usage() as f64 / sys.cpus().len().max(1) as f64);
        }
    }
    row("idle-CPU kern", cpu.iter().sum::<f64>() / cpu.len().max(1) as f64, 2.0, "%");
    println!("60 fps GUI: meten met de Playwright-check (`app/`), niet via deze CLI.");
    Ok(ok)
}

/// `vortex soak`: gesimuleerd gebruik + RSS-trend per proces. 🔒 24 uur alleen op commando.
pub async fn soak(base: &str, minutes: u64, out_dir: &std::path::Path) -> anyhow::Result<bool> {
    let api = Api { c: reqwest::Client::new(), base: base.into() };
    let st = api.get("/api/status").await.context("kern niet bereikbaar")?;
    let pid = st["pid"].as_u64().unwrap_or(0) as u32;
    std::fs::create_dir_all(out_dir)?;
    let file = out_dir.join(format!("soak-{}.csv", vortex_types::now().format("%Y%m%d-%H%M%S")));
    let mut csv = std::fs::File::create(&file)?;
    writeln!(csv, "seconden,rss_mib")?;
    let mut sys = System::new();
    let start = Instant::now();
    let end = start + Duration::from_secs(minutes * 60);
    let sid = api.post("/api/sessions", json!({})).await?["id"].as_str().unwrap_or_default().to_string();
    let mut samples: Vec<(f64, f64)> = Vec::new();
    let mut i = 0u64;
    while Instant::now() < end {
        if i.is_multiple_of(3) {
            let _ = api
                .post(
                    &format!("/api/sessions/{sid}/messages"),
                    json!({"text": format!("Soak-bericht {i}: vat kort samen wat je weet."), "mode": "quick"}),
                )
                .await;
            let _ = api.post("/api/memory/search", json!({"text": format!("soak {i}")})).await;
        }
        let p = Pid::from_u32(pid);
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::new().with_memory());
        let rss = sys.process(p).map(|x| x.memory() as f64 / 1048576.0).unwrap_or(f64::NAN);
        let t = start.elapsed().as_secs_f64();
        writeln!(csv, "{t:.0},{rss:.1}")?;
        samples.push((t, rss));
        i += 1;
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    // Lineaire trend (MiB per uur) over de tweede helft (opwarmen telt niet).
    let half: Vec<_> = samples.iter().skip(samples.len() / 2).copied().collect();
    let n = half.len() as f64;
    let (sx, sy) = half.iter().fold((0.0, 0.0), |a, (x, y)| (a.0 + x, a.1 + y));
    let (mx, my) = (sx / n, sy / n);
    let (num, den) = half.iter().fold((0.0, 0.0), |a, (x, y)| (a.0 + (x - mx) * (y - my), a.1 + (x - mx).powi(2)));
    let slope_h = if den > 0.0 { num / den * 3600.0 } else { 0.0 };
    let flat = slope_h.abs() <= 5.0;
    println!(
        "RSS-trend: {slope_h:+.1} MiB/uur → {} (data: {})",
        if flat { "VLAK (OK)" } else { "STIJGT (ROOD)" },
        file.display()
    );
    Ok(flat)
}
