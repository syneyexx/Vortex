//! Drukmeters + idle-detectie → condities voor de Dirigent (W04.01).
//! RAM/CPU via sysinfo, GPU via nvidia-smi, gebruiker-idle via het OS (Windows: GetLastInputInfo).
//! "Externe GPU-last" (bv. een game): GPU-gebruik terwijl Vortex zelf geen GPU-taak heeft.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio_util::sync::CancellationToken;

use crate::dirigent::{Conditions, Dirigent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    pub interval: Duration,
    /// Na zoveel seconden zonder invoer telt de gebruiker als idle.
    pub idle_after_secs: u64,
    /// Als het OS geen idle-tijd geeft (Linux-server): idle aannemen?
    pub assume_idle_when_unknown: bool,
    pub ram_high_pct: f32,
    pub ram_critical_pct: f32,
    pub vram_critical_pct: f32,
    /// GPU-gebruik (%) zonder eigen GPU-taak dat als "externe last" telt.
    pub external_gpu_util_pct: u32,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(2),
            idle_after_secs: 300,
            assume_idle_when_unknown: true,
            ram_high_pct: 85.0,
            ram_critical_pct: 93.0,
            vram_critical_pct: 97.0,
            external_gpu_util_pct: 30,
        }
    }
}

/// Eén meting.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sample {
    pub ram_used_pct: f32,
    pub cpu_pct: f32,
    pub gpu_util_pct: Vec<u32>,
    pub vram_used_pct: Vec<f32>,
    pub idle_secs: Option<u64>,
}

fn gpu_sample() -> (Vec<u32>, Vec<f32>) {
    let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=utilization.gpu,memory.used,memory.total", "--format=csv,noheader,nounits"])
        .output()
    else {
        return (vec![], vec![]);
    };
    let mut util = Vec::new();
    let mut vram = Vec::new();
    for l in String::from_utf8_lossy(&out.stdout).lines() {
        let f: Vec<f32> = l.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if f.len() == 3 {
            util.push(f[0] as u32);
            vram.push(100.0 * f[1] / f[2].max(1.0));
        }
    }
    (util, vram)
}

/// Bepaal condities uit een meting. `vortex_gpu_busy` = Vortex heeft zelf een GPU-taak lopen.
pub fn conditions_from(s: &Sample, cfg: &SamplerConfig, vortex_gpu_busy: bool) -> Conditions {
    let user_idle = match s.idle_secs {
        Some(i) => i >= cfg.idle_after_secs,
        None => cfg.assume_idle_when_unknown,
    };
    let external_gpu_load = !vortex_gpu_busy && s.gpu_util_pct.iter().any(|u| *u >= cfg.external_gpu_util_pct);
    let vram_crit = s.vram_used_pct.first().is_some_and(|v| *v >= cfg.vram_critical_pct) && !vortex_gpu_busy;
    let pressure = if s.ram_used_pct >= cfg.ram_critical_pct || vram_crit {
        2
    } else if s.ram_used_pct >= cfg.ram_high_pct {
        1
    } else {
        0
    };
    Conditions { user_idle, external_gpu_load, pressure }
}

/// Draai de sampler en voed de Dirigent tot `stop`.
pub fn spawn(d: Dirigent, cfg: SamplerConfig, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sys = System::new_with_specifics(
            RefreshKind::new()
                .with_memory(MemoryRefreshKind::new().with_ram())
                .with_cpu(CpuRefreshKind::new().with_cpu_usage()),
        );
        loop {
            sys.refresh_memory();
            sys.refresh_cpu_usage();
            let (gpu_util_pct, vram_used_pct) = tokio::task::spawn_blocking(gpu_sample).await.unwrap_or_default();
            let s = Sample {
                ram_used_pct: 100.0 * sys.used_memory() as f32 / sys.total_memory().max(1) as f32,
                cpu_pct: sys.global_cpu_usage(),
                gpu_util_pct,
                vram_used_pct,
                idle_secs: crate::platform::idle_secs(),
            };
            let busy = d.snapshot().used.main_gpu > 0 || d.snapshot().used.aux_gpu_mib > 0;
            d.set_conditions(conditions_from(&s, &cfg, busy));
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(cfg.interval) => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_running_is_external_load_only_when_vortex_idle() {
        let cfg = SamplerConfig::default();
        let s = Sample {
            gpu_util_pct: vec![85, 0],
            vram_used_pct: vec![60.0, 10.0],
            idle_secs: Some(10),
            ..Default::default()
        };
        assert!(conditions_from(&s, &cfg, false).external_gpu_load);
        assert!(!conditions_from(&s, &cfg, true).external_gpu_load, "eigen werk is geen externe last");
        assert!(!conditions_from(&s, &cfg, false).user_idle);
    }

    #[test]
    fn pressure_levels() {
        let cfg = SamplerConfig::default();
        let mk = |r: f32| Sample { ram_used_pct: r, idle_secs: Some(9999), ..Default::default() };
        assert_eq!(conditions_from(&mk(50.0), &cfg, false).pressure, 0);
        assert_eq!(conditions_from(&mk(88.0), &cfg, false).pressure, 1);
        assert_eq!(conditions_from(&mk(95.0), &cfg, false).pressure, 2);
        assert!(conditions_from(&mk(50.0), &cfg, false).user_idle);
    }
}
