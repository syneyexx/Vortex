//! Record/replay voor tooltests (W17.01): een "tape" neemt echte toolaanroepen op (invoer +
//! resultaat, JSONL) en speelt ze later af als simulatie — deterministisch, offline, zonder
//! bijwerkingen. Modus `Replay` is strikt (niet opgenomen = fout); `ReplayOrLive` valt terug.
// VX:W17.01

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::action::ActionResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapeMode {
    Record,
    Replay,
    ReplayOrLive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TapeEntry {
    pub tool: String,
    pub key: String,
    pub input: serde_json::Value,
    pub result: ActionResult,
}

#[derive(Debug)]
pub struct Tape {
    pub mode: TapeMode,
    path: Option<PathBuf>,
    entries: Mutex<Vec<TapeEntry>>,
    /// Bij afspelen: welke opnames al gebruikt zijn (zelfde aanroep twee keer → twee opnames).
    used: Mutex<Vec<bool>>,
    misses: Mutex<Vec<String>>,
}

impl Tape {
    pub fn record(path: Option<PathBuf>) -> Self {
        Self {
            mode: TapeMode::Record,
            path,
            entries: Mutex::new(vec![]),
            used: Mutex::new(vec![]),
            misses: Mutex::new(vec![]),
        }
    }

    pub fn load(path: &Path, mode: TapeMode) -> std::io::Result<Self> {
        let entries: Vec<TapeEntry> =
            std::fs::read_to_string(path)?.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let n = entries.len();
        Ok(Self {
            mode,
            path: Some(path.to_path_buf()),
            entries: Mutex::new(entries),
            used: Mutex::new(vec![false; n]),
            misses: Mutex::new(vec![]),
        })
    }

    pub fn entries(&self) -> Vec<TapeEntry> {
        self.entries.lock().clone()
    }

    pub fn misses(&self) -> Vec<String> {
        self.misses.lock().clone()
    }

    /// Opname zoeken (eerste ongebruikte met zelfde tool + invoersleutel).
    pub fn lookup(&self, tool: &str, key: &str) -> Option<ActionResult> {
        let es = self.entries.lock();
        let mut used = self.used.lock();
        if used.len() < es.len() {
            used.resize(es.len(), false);
        }
        let i = es.iter().enumerate().position(|(i, e)| !used[i] && e.tool == tool && e.key == key)?;
        used[i] = true;
        let mut r = es[i].result.clone();
        r.replayed = true;
        Some(r)
    }

    pub fn miss(&self, tool: &str, key: &str) {
        self.misses.lock().push(format!("{tool}:{key}"));
    }

    pub fn push(&self, tool: &str, key: &str, input: &serde_json::Value, result: &ActionResult) {
        let e = TapeEntry { tool: tool.into(), key: key.into(), input: input.clone(), result: result.clone() };
        if let Some(p) = &self.path {
            use std::io::Write;
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{}", serde_json::to_string(&e).unwrap_or_default());
            }
        }
        self.entries.lock().push(e);
    }
}
