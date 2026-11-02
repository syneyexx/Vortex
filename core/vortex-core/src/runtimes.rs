//! Twee tokio-runtimes: interactief (CCD0, lage latency) en achtergrond (CCD1). Worker-threads
//! worden op hun cores vastgezet. Processen (Python-worker, crawlers) krijgen affiniteit en
//! prioriteit via [`place_process`].
// VX:W04.02

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::platform::{ProcPriority, set_affinity, set_priority};

/// Welke logische cores bij welke kant horen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuLayout {
    pub interactive: Vec<usize>,
    pub background: Vec<usize>,
}

impl CpuLayout {
    /// Ryzen 9 5950X: 2 CCD's van 8 cores (16 threads elk). Windows nummert SMT-paren naast
    /// elkaar (0,1 = core 0), Linux eerst alle cores en dan de SMT-broers (n+16).
    pub fn detect() -> Self {
        let n = core_affinity::get_core_ids().map(|c| c.len()).unwrap_or(1).max(1);
        Self::for_count(n, cfg!(windows))
    }

    pub fn for_count(n: usize, windows_order: bool) -> Self {
        if n < 4 {
            let all: Vec<usize> = (0..n).collect();
            return Self { interactive: all.clone(), background: all };
        }
        let half = n / 2;
        if windows_order || !n.is_multiple_of(4) {
            return Self { interactive: (0..half).collect(), background: (half..n).collect() };
        }
        // Linux-volgorde: [0..q) en [2q..3q) = CCD0, [q..2q) en [3q..4q) = CCD1.
        let q = n / 4;
        let ccd0 = (0..q).chain(2 * q..3 * q).collect();
        let ccd1 = (q..2 * q).chain(3 * q..4 * q).collect();
        Self { interactive: ccd0, background: ccd1 }
    }
}

/// De twee runtimes.
#[derive(Debug)]
pub struct Runtimes {
    pub interactive: tokio::runtime::Runtime,
    pub background: tokio::runtime::Runtime,
    pub layout: CpuLayout,
}

fn build(name: &str, cores: Vec<usize>, threads: usize, blocking: usize) -> Result<tokio::runtime::Runtime> {
    let ids: Vec<core_affinity::CoreId> =
        core_affinity::get_core_ids().unwrap_or_default().into_iter().filter(|c| cores.contains(&c.id)).collect();
    let ids = Arc::new(ids);
    let next = Arc::new(AtomicUsize::new(0));
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads.max(1))
        .max_blocking_threads(blocking.max(1))
        .thread_name(name.to_string())
        .enable_all()
        .on_thread_start(move || {
            if !ids.is_empty() {
                let i = next.fetch_add(1, Ordering::Relaxed) % ids.len();
                core_affinity::set_for_current(ids[i]);
            }
        })
        .build()
        .map_err(CoreError::Io)
}

impl Runtimes {
    /// Bouw beide runtimes. Begrensde blocking-pools (geen onbeperkte threads).
    pub fn build(layout: CpuLayout) -> Result<Self> {
        let interactive = build("vx-interactive", layout.interactive.clone(), layout.interactive.len().min(8), 16)?;
        let background = build("vx-background", layout.background.clone(), layout.background.len().min(16), 32)?;
        Ok(Self { interactive, background, layout })
    }
}

/// Plaats een proces op de juiste CCD met de juiste prioriteit. Fouten worden gemeld, niet fataal.
pub fn place_process(pid: u32, cores: &[usize], priority: ProcPriority) -> Vec<String> {
    let mut warn = Vec::new();
    if let Err(e) = set_affinity(pid, cores) {
        warn.push(format!("affiniteit: {e}"));
    }
    if let Err(e) = set_priority(pid, priority) {
        warn.push(format!("prioriteit: {e}"));
    }
    warn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_5950x_windows_and_linux() {
        let w = CpuLayout::for_count(32, true);
        assert_eq!(w.interactive, (0..16).collect::<Vec<_>>());
        assert_eq!(w.background, (16..32).collect::<Vec<_>>());
        let l = CpuLayout::for_count(32, false);
        assert_eq!(l.interactive[..8], (0..8).collect::<Vec<_>>()[..]);
        assert_eq!(l.interactive[8..], (16..24).collect::<Vec<_>>()[..]);
        assert!(l.background.contains(&8) && l.background.contains(&31));
        assert!(l.interactive.iter().all(|c| !l.background.contains(c)));
    }

    #[test]
    fn small_machine_shares_cores() {
        let l = CpuLayout::for_count(2, false);
        assert_eq!(l.interactive, l.background);
    }

    #[test]
    fn runtimes_run_tasks_on_both_sides() -> Result<()> {
        let rt = Runtimes::build(CpuLayout::detect())?;
        let a = rt.interactive.block_on(async { 1 + 1 });
        let b = rt.background.block_on(async { tokio::task::spawn(async { 40 + 2 }).await.unwrap_or(0) });
        assert_eq!((a, b), (2, 42));
        Ok(())
    }
}
