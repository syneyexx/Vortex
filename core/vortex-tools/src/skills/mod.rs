//! Skills (W08.13): eigen Vortex-formaat (superset van Claude Skills), converters ⇄ Claude
//! Skills ⇄ Cursor rules ⇄ AGENTS.md, progressive disclosure (naam+beschrijving in context,
//! inhoud via `load_skill`), selectie (trefwoorden + embeddings + globs + alwaysApply), scripts in
//! de sandbox, hot-reload uit `config/skills/`, statistiek (succes, uitzetten).
// VX:W08.13

pub mod convert;
pub mod frontmatter;
pub mod propose;
pub mod tools;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use vortex_core::EventLog;
use vortex_types::{Event, EventKind, TraceId};

/// Eén skill.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Instructies (markdown).
    pub body: String,
    /// Cursor-stijl globs: actief als een betrokken bestand matcht.
    #[serde(default)]
    pub globs: Vec<String>,
    /// Altijd in de context (Cursor `alwaysApply`).
    #[serde(default)]
    pub always_apply: bool,
    /// Extra trefwoorden voor selectie.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Tools die de skill gebruikt (Claude `allowed-tools`).
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default = "v1")]
    pub version: String,
    /// vortex | claude | cursor | agents | proposed
    #[serde(default)]
    pub origin: String,
    /// Map van de skill (scripts/resources).
    #[serde(default)]
    pub dir: Option<PathBuf>,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    /// Overige frontmatter-velden (blijven behouden bij export).
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}
fn v1() -> String {
    "1".into()
}

impl Skill {
    /// Naam normaliseren naar Claude-stijl (kleine letters, koppeltekens, ≤64).
    pub fn slug(s: &str) -> String {
        let mut out = String::new();
        for c in s.trim().to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c);
            } else if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        }
        out.trim_end_matches('-').chars().take(64).collect()
    }
}

/// Statistiek per skill.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillStats {
    pub uses: i64,
    pub successes: i64,
    pub failures: i64,
    pub status: String,
}

/// Selectieresultaat.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Selected {
    pub name: String,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug)]
pub struct SkillStore {
    dir: PathBuf,
    skills: RwLock<BTreeMap<String, Skill>>,
    log: Option<EventLog>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

const STOP: &[&str] = &[
    "de", "het", "een", "en", "of", "van", "in", "op", "te", "is", "met", "voor", "aan", "die", "dat", "dit", "je",
    "ik", "the", "a", "an", "to", "of", "and", "or", "for", "with", "on", "how", "hoe", "wat", "kan", "mijn", "zijn",
    "er", "als", "bij", "om", "naar", "uit", "use", "when", "this",
];

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(|w| w.trim_end_matches('s').to_string())
        .collect()
}

impl SkillStore {
    pub fn new(dir: impl Into<PathBuf>, log: Option<EventLog>) -> Arc<Self> {
        let s =
            Arc::new(Self { dir: dir.into(), skills: RwLock::new(BTreeMap::new()), log, watcher: Mutex::new(None) });
        let _ = std::fs::create_dir_all(&s.dir);
        s.reload();
        s
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Alles opnieuw inlezen. Geeft het aantal skills.
    pub fn reload(&self) -> usize {
        let mut map = BTreeMap::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut es: Vec<_> = rd.flatten().collect();
            es.sort_by_key(|e| e.file_name());
            for e in es {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('_') || name.starts_with('.') {
                    continue;
                }
                let found = if p.is_dir() || name.ends_with(".mdc") || name == "AGENTS.md" {
                    convert::import(&p).unwrap_or_default()
                } else {
                    vec![]
                };
                for s in found {
                    map.insert(s.name.clone(), s);
                }
            }
        }
        let n = map.len();
        *self.skills.write() = map;
        if let Some(l) = &self.log {
            let _ = l.append(Event::new(EventKind::SkillsReloaded, TraceId::new(), serde_json::json!({"count": n})));
        }
        n
    }

    /// Hot-reload: map bewaken; bij wijziging opnieuw inlezen.
    pub fn watch(self: &Arc<Self>) -> Result<(), String> {
        use notify::Watcher;
        let weak = Arc::downgrade(self);
        let pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // Naloop-debounce: na de laatste wijziging in een burst 250 ms wachten, dan herladen.
            if res.is_ok() && !pending.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let (weak, pending) = (weak.clone(), pending.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    pending.store(false, std::sync::atomic::Ordering::SeqCst);
                    if let Some(s) = weak.upgrade() {
                        s.reload();
                    }
                });
            }
        })
        .map_err(|e| e.to_string())?;
        w.watch(&self.dir, notify::RecursiveMode::Recursive).map_err(|e| e.to_string())?;
        *self.watcher.lock() = Some(w);
        Ok(())
    }

    pub fn list(&self) -> Vec<Skill> {
        self.skills.read().values().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<Skill> {
        self.skills.read().get(name).cloned()
    }

    fn disabled(&self) -> Vec<String> {
        let Some(l) = &self.log else { return vec![] };
        l.db()
            .read(|c| {
                let mut st = c.prepare("SELECT name FROM skill_stats WHERE status = 'disabled'")?;
                let v = st.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .unwrap_or_default()
    }

    /// Progressive disclosure: alleen naam + beschrijving (uitgezette skills niet).
    pub fn briefs_text(&self) -> String {
        let off = self.disabled();
        self.skills
            .read()
            .values()
            .filter(|s| !off.contains(&s.name))
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Kies skills voor deze taak. `semantic` = embedding-gelijkenis per skillnaam (0..1) van de
    /// aanroeper (de server heeft de embedder); `files` = betrokken bestanden (voor globs).
    pub fn select(&self, query: &str, files: &[String], semantic: &HashMap<String, f64>, k: usize) -> Vec<Selected> {
        let q: Vec<String> = words(query);
        let off = self.disabled();
        let mut out: Vec<Selected> = Vec::new();
        for s in self.skills.read().values() {
            if off.contains(&s.name) {
                continue;
            }
            if s.always_apply {
                out.push(Selected { name: s.name.clone(), score: 1.0, reason: "altijd actief".into() });
                continue;
            }
            if let Some(f) = files.iter().find(|f| s.globs.iter().any(|g| crate::glob::matches(g, f))) {
                out.push(Selected { name: s.name.clone(), score: 0.95, reason: format!("glob past op {f}") });
                continue;
            }
            let hay: Vec<String> =
                words(&format!("{} {} {}", s.name.replace('-', " "), s.description, s.triggers.join(" ")));
            let hits = q.iter().filter(|w| hay.contains(w)).count();
            let kw = if q.is_empty() { 0.0 } else { hits as f64 / (q.len().min(6) as f64) };
            let trig = s.triggers.iter().any(|t| query.to_lowercase().contains(&t.to_lowercase()));
            let sem = semantic.get(&s.name).copied().unwrap_or(0.0);
            let score = (0.5 * kw.min(1.0) + 0.5 * sem + if trig { 0.4 } else { 0.0 }).min(1.0);
            if score >= 0.3 {
                let mut why = Vec::new();
                if hits > 0 {
                    why.push(format!("{hits} trefwoord(en)"));
                }
                if trig {
                    why.push("trigger".into());
                }
                if sem > 0.0 {
                    why.push(format!("semantisch {sem:.2}"));
                }
                out.push(Selected { name: s.name.clone(), score, reason: why.join(", ") });
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(k);
        out
    }

    /// Installeer (schrijf in de skills-map als Vortex-skill). Hot-reload pakt hem op.
    pub fn install(&self, s: &Skill) -> Result<PathBuf, String> {
        let p = convert::write_vortex(s, &self.dir).map_err(|e| e.to_string())?;
        self.reload();
        Ok(p)
    }

    pub fn remove(&self, name: &str) -> Result<(), String> {
        let p = self.dir.join(name);
        if p.is_dir() {
            std::fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        }
        self.reload();
        Ok(())
    }

    /// Gebruik registreren (bij laden).
    pub fn record_use(&self, name: &str) {
        self.bump(name, "uses");
        if let Some(l) = &self.log {
            let _ = l.append(Event::new(EventKind::SkillLoaded, TraceId::new(), serde_json::json!({"name": name})));
        }
    }

    /// Uitkomst registreren; slechte skills (≥5 keer gebruikt, <30 % succes) gaan uit.
    pub fn record_outcome(&self, name: &str, success: bool) {
        self.bump(name, if success { "successes" } else { "failures" });
        if let Some(st) = self.stats(name)
            && st.successes + st.failures >= 5
            && (st.successes as f64) / ((st.successes + st.failures) as f64) < 0.3
        {
            let _ = self.set_status(name, "disabled");
        }
    }

    pub fn set_status(&self, name: &str, status: &str) -> Result<(), String> {
        let Some(l) = &self.log else { return Ok(()) };
        let v = self.get(name).map(|s| s.version).unwrap_or_else(v1);
        let now = vortex_types::now().to_rfc3339();
        l.db()
            .write(|tx| {
                tx.execute(
                    "INSERT INTO skill_stats(name, version, status, updated_at) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(name) DO UPDATE SET status = ?3, updated_at = ?4",
                    params![name, v, status, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    fn bump(&self, name: &str, col: &str) {
        let Some(l) = &self.log else { return };
        let v = self.get(name).map(|s| s.version).unwrap_or_else(v1);
        let now = vortex_types::now().to_rfc3339();
        let sql = format!(
            "INSERT INTO skill_stats(name, version, {col}, updated_at) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(name) DO UPDATE SET {col} = {col} + 1, version = ?2, updated_at = ?3"
        );
        let _ = l.db().write(|tx| {
            tx.execute(&sql, params![name, v, now])?;
            Ok(())
        });
    }

    pub fn stats(&self, name: &str) -> Option<SkillStats> {
        let l = self.log.as_ref()?;
        l.db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT uses, successes, failures, status FROM skill_stats WHERE name = ?1",
                    [name],
                    |r| {
                        Ok(SkillStats { uses: r.get(0)?, successes: r.get(1)?, failures: r.get(2)?, status: r.get(3)? })
                    },
                )
                .ok())
            })
            .ok()
            .flatten()
    }
}
