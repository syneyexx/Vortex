//! Config-systeem: versies, hot-reload uit `config/`, lopende runs houden hun eigen versie
//! (ze houden een `Arc<ConfigSnapshot>` vast). Elke versie wordt bewaard (geschiedenis/terugzetten).
//!
//! Indeling: `config/*.toml` → tabel per bestandsnaam (`models.toml` → `models.*`);
//! tekstbestanden in submappen (`prompts/`, `roles/`, `skills/`) → `files["prompts/system.md"]`.
// VX:W01.13

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use parking_lot::RwLock;
use rusqlite::params;
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, TraceId};

use crate::error::{CoreError, Result};
use crate::events::EventLog;

/// Onveranderlijke momentopname van alle config.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSnapshot {
    pub version: String,
    pub values: toml::Table,
    pub files: BTreeMap<String, String>,
}

impl ConfigSnapshot {
    pub fn empty() -> Self {
        Self { version: "cfg-empty".into(), values: toml::Table::new(), files: BTreeMap::new() }
    }

    /// Waarde op pad `a.b.c` als type `T`.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let mut cur: Option<&toml::Value> = None;
        for (i, part) in path.split('.').enumerate() {
            cur = if i == 0 { self.values.get(part) } else { cur.and_then(|v| v.get(part)) };
            if cur.is_none() {
                return Ok(None);
            }
        }
        cur.map(|v| v.clone().try_into::<T>().map_err(|e| CoreError::Config(format!("{path}: {e}")))).transpose()
    }

    /// Tekstbestand (prompt, rol, skill).
    pub fn file(&self, rel: &str) -> Option<&str> {
        self.files.get(rel).map(String::as_str)
    }
}

/// Stabiele 64-bit FNV-1a hash (onafhankelijk van Rust-versie of proces).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn load_dir(dir: &Path) -> Result<ConfigSnapshot> {
    let mut values = toml::Table::new();
    let mut files = BTreeMap::new();
    let mut canonical = String::new();
    if !dir.exists() {
        return Ok(ConfigSnapshot::empty());
    }
    let mut entries: Vec<PathBuf> = Vec::new();
    collect(dir, &mut entries)?;
    entries.sort();
    for p in entries {
        let rel =
            p.strip_prefix(dir).map_err(|e| CoreError::Config(e.to_string()))?.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&p)?;
        canonical.push_str(&rel);
        canonical.push('\0');
        canonical.push_str(&text);
        canonical.push('\0');
        let is_top_toml = !rel.contains('/') && rel.ends_with(".toml");
        if is_top_toml {
            let table: toml::Table = text.parse().map_err(|e| CoreError::Config(format!("{rel}: {e}")))?;
            let stem = rel.trim_end_matches(".toml").to_string();
            values.insert(stem, toml::Value::Table(table));
        } else {
            files.insert(rel, text);
        }
    }
    let version = format!("cfg-{:016x}", fnv1a64(canonical.as_bytes()));
    Ok(ConfigSnapshot { version, values, files })
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect(&p, out)?;
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("toml" | "md" | "mdc" | "txt" | "json" | "yaml" | "yml")
        ) {
            out.push(p);
        }
    }
    Ok(())
}

/// Beheert de huidige config en de geschiedenis.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    dir: PathBuf,
    current: Arc<RwLock<Arc<ConfigSnapshot>>>,
    log: EventLog,
}

impl ConfigManager {
    /// Laad `dir`. Ongeldige config bij opstarten = fout (start niet met kapotte instellingen).
    pub fn load(dir: impl Into<PathBuf>, log: EventLog) -> Result<Self> {
        let dir = dir.into();
        let snap = load_dir(&dir)?;
        let m = Self { dir, current: Arc::new(RwLock::new(Arc::new(snap.clone()))), log };
        m.persist(&snap)?;
        Ok(m)
    }

    /// Huidige versie. Een run houdt deze `Arc` vast = eigen versie tot de run klaar is.
    pub fn current(&self) -> Arc<ConfigSnapshot> {
        self.current.read().clone()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Opnieuw inlezen. Geeft `true` als er een nieuwe versie is. Ongeldig = oude blijft actief.
    pub fn reload(&self) -> Result<bool> {
        let snap = match load_dir(&self.dir) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "config ongeldig; vorige versie blijft actief");
                return Err(e);
            }
        };
        if snap.version == self.current.read().version {
            return Ok(false);
        }
        let old = self.current.read().version.clone();
        self.persist(&snap)?;
        self.log.append(Event::new(
            EventKind::ConfigReloaded,
            TraceId::new(),
            serde_json::json!({"from": old, "to": snap.version}),
        ))?;
        tracing::info!(from = %old, to = %snap.version, "config herladen");
        *self.current.write() = Arc::new(snap);
        Ok(true)
    }

    fn persist(&self, snap: &ConfigSnapshot) -> Result<()> {
        let content = serde_json::json!({
            "values": serde_json::to_value(&snap.values)?,
            "files": snap.files,
        });
        self.log.db().write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO config_versions(version, hash, ts, content) VALUES (?1, ?1, ?2, ?3)",
                params![snap.version, vortex_types::now().to_rfc3339(), content.to_string()],
            )?;
            Ok(())
        })
    }

    /// Versiegeschiedenis (nieuwste eerst): (versie, tijdstip).
    pub fn history(&self) -> Result<Vec<(String, String)>> {
        self.log.db().read(|c| {
            let mut st = c.prepare("SELECT version, ts FROM config_versions ORDER BY ts DESC")?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Hot-reload: let op wijzigingen in `dir` tot `stop` (met korte debounce).
    pub fn watch(&self, stop: CancellationToken) -> Result<tokio::task::JoinHandle<()>> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                let _ = tx.try_send(());
            }
        })
        .map_err(|e| CoreError::Config(e.to_string()))?;
        std::fs::create_dir_all(&self.dir)?;
        watcher.watch(&self.dir, RecursiveMode::Recursive).map_err(|e| CoreError::Config(e.to_string()))?;
        let me = self.clone();
        Ok(tokio::spawn(async move {
            let _watcher = watcher; // leeft zolang de taak leeft
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    msg = rx.recv() => {
                        if msg.is_none() { break; }
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        while rx.try_recv().is_ok() {}
                        let _ = me.reload();
                    }
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};

    fn mgr(dir: &Path) -> Result<ConfigManager> {
        ConfigManager::load(dir, EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?)))
    }

    #[test]
    fn loads_tables_and_files_and_versions_change_with_content() -> Result<()> {
        let d = tempfile::tempdir()?;
        std::fs::write(d.path().join("models.toml"), "default = \"gpt-oss-20b\"\n[ctx]\nmax = 8192\n")?;
        std::fs::create_dir(d.path().join("prompts"))?;
        std::fs::write(d.path().join("prompts/system.md"), "Je bent Vortex.")?;
        let m = mgr(d.path())?;
        let s1 = m.current();
        assert_eq!(s1.get::<String>("models.default")?, Some("gpt-oss-20b".into()));
        assert_eq!(s1.get::<i64>("models.ctx.max")?, Some(8192));
        assert_eq!(s1.get::<i64>("models.nope")?, None);
        assert_eq!(s1.file("prompts/system.md"), Some("Je bent Vortex."));
        assert!(!m.reload()?, "zelfde inhoud = zelfde versie");
        std::fs::write(d.path().join("prompts/system.md"), "Je bent Vortex v2.")?;
        assert!(m.reload()?);
        let s2 = m.current();
        assert_ne!(s1.version, s2.version);
        assert_eq!(s1.file("prompts/system.md"), Some("Je bent Vortex."), "oude snapshot blijft voor lopende run");
        assert_eq!(m.history()?.len(), 2);
        Ok(())
    }

    #[test]
    fn invalid_config_keeps_previous_version() -> Result<()> {
        let d = tempfile::tempdir()?;
        std::fs::write(d.path().join("a.toml"), "x = 1\n")?;
        let m = mgr(d.path())?;
        let v = m.current().version.clone();
        std::fs::write(d.path().join("a.toml"), "x = = kapot")?;
        assert!(m.reload().is_err());
        assert_eq!(m.current().version, v);
        Ok(())
    }

    #[tokio::test]
    async fn watcher_hot_reloads() -> Result<()> {
        let d = tempfile::tempdir()?;
        std::fs::write(d.path().join("a.toml"), "x = 1\n")?;
        let m = mgr(d.path())?;
        let stop = CancellationToken::new();
        let h = m.watch(stop.clone())?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(d.path().join("a.toml"), "x = 2\n")?;
        let mut ok = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if m.current().get::<i64>("a.x")? == Some(2) {
                ok = true;
                break;
            }
        }
        stop.cancel();
        let _ = h.await;
        assert!(ok, "hot-reload binnen 2,5 s");
        Ok(())
    }

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
