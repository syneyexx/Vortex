//! Performance-harnas (W06.11) — 🔒 de volledige suite draait alleen op commando ("GROTE TESTEN
//! NU"). Hier staan de doelen en de meetfuncties; losse snelle metingen mogen tussendoor.
// VX:W06.11

use std::time::{Duration, Instant};

use serde::Serialize;

/// Startwaarden uit VAULT/modules/orkestrator-performance.md (TTFT na meting in W00).
#[derive(Debug, Clone, Serialize)]
pub struct PerfTargets {
    pub retrieval_p95_ms: f64,
    pub stop_latency_ms: f64,
    pub idle_cpu_pct: f64,
    pub ttft_ms: Option<f64>,
    pub gui_fps: f64,
}

impl Default for PerfTargets {
    fn default() -> Self {
        Self { retrieval_p95_ms: 150.0, stop_latency_ms: 500.0, idle_cpu_pct: 2.0, ttft_ms: None, gui_fps: 60.0 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    pub name: String,
    pub samples: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub target_ms: Option<f64>,
    pub ok: Option<bool>,
}

/// Meet `n` keer een async operatie en geef percentielen.
pub async fn measure<F, Fut>(name: &str, n: usize, target_ms: Option<f64>, mut f: F) -> Measurement
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut d: Vec<Duration> = Vec::with_capacity(n);
    for i in 0..n {
        let t = Instant::now();
        f(i).await;
        d.push(t.elapsed());
    }
    d.sort_unstable();
    let ms = |x: Duration| x.as_secs_f64() * 1000.0;
    let pick = |p: f64| d.get(((d.len() as f64 * p).ceil() as usize).saturating_sub(1)).copied().unwrap_or_default();
    let p95 = ms(pick(0.95));
    Measurement {
        name: name.into(),
        samples: n,
        p50_ms: ms(pick(0.5)),
        p95_ms: p95,
        max_ms: d.last().map(|x| ms(*x)).unwrap_or(0.0),
        target_ms,
        ok: target_ms.map(|t| p95 <= t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn percentiles() {
        let m = measure("slaap", 20, Some(50.0), |i| async move {
            tokio::time::sleep(Duration::from_millis(if i == 19 { 20 } else { 1 })).await
        })
        .await;
        assert!(m.p95_ms < m.max_ms + 1e-9);
        assert_eq!(m.ok, Some(true));
    }
}
