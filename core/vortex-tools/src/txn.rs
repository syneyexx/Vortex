//! Transactionele bestandswijzigingen (W08.04): schrijftools zetten wijzigingen klaar in een
//! staging-laag; daarna preview (diff) → valideren (postcondities) → atomair toepassen
//! (temp-bestand + rename, met rollback als een latere schrijfactie faalt).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

#[derive(Debug, Clone)]
struct Staged {
    original: Option<String>,
    new: Option<String>,
}

#[derive(Debug, Default)]
pub struct FileTxn {
    staged: Mutex<BTreeMap<PathBuf, Staged>>,
}

impl FileTxn {
    pub fn new() -> Self {
        Self::default()
    }

    /// Gestaged inhoud: `Some(Some(x))` = nieuwe inhoud, `Some(None)` = verwijderd, `None` = niet aangeraakt.
    pub fn staged(&self, p: &Path) -> Option<Option<String>> {
        self.staged.lock().get(p).map(|s| s.new.clone())
    }

    /// Huidige inhoud zoals de transactie hem ziet.
    pub fn read(&self, p: &Path) -> std::io::Result<String> {
        match self.staged(p) {
            Some(Some(s)) => Ok(s),
            Some(None) => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "verwijderd in deze transactie")),
            None => std::fs::read_to_string(p),
        }
    }

    pub fn write(&self, p: &Path, content: String) {
        let mut g = self.staged.lock();
        let e =
            g.entry(p.to_path_buf()).or_insert_with(|| Staged { original: std::fs::read_to_string(p).ok(), new: None });
        e.new = Some(content);
    }

    pub fn delete(&self, p: &Path) {
        let mut g = self.staged.lock();
        let e =
            g.entry(p.to_path_buf()).or_insert_with(|| Staged { original: std::fs::read_to_string(p).ok(), new: None });
        e.new = None;
    }

    pub fn is_empty(&self) -> bool {
        self.staged.lock().is_empty()
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.staged.lock().keys().cloned().collect()
    }

    /// Diff van alle klaargezette wijzigingen.
    pub fn preview(&self) -> String {
        self.staged
            .lock()
            .iter()
            .map(|(p, s)| {
                crate::diff::unified(
                    &p.display().to_string(),
                    s.original.as_deref().unwrap_or(""),
                    s.new.as_deref().unwrap_or(""),
                    3,
                )
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Atomair toepassen. Faalt er één → alles terug naar de oorspronkelijke toestand.
    pub fn commit(&self) -> std::io::Result<usize> {
        let items: Vec<(PathBuf, Staged)> = self.staged.lock().iter().map(|(p, s)| (p.clone(), s.clone())).collect();
        let mut done: Vec<(PathBuf, Option<String>)> = Vec::new();
        for (p, s) in &items {
            let r = match &s.new {
                Some(c) => atomic_write(p, c),
                None => match std::fs::remove_file(p) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            if let Err(e) = r {
                for (dp, orig) in done.into_iter().rev() {
                    let _ = match orig {
                        Some(o) => atomic_write(&dp, &o),
                        None => std::fs::remove_file(&dp),
                    };
                }
                return Err(e);
            }
            done.push((p.clone(), s.original.clone()));
        }
        self.staged.lock().clear();
        Ok(items.len())
    }

    pub fn rollback(&self) {
        self.staged.lock().clear();
    }
}

/// Schrijf via temp-bestand in dezelfde map + rename (nooit een half bestand).
pub fn atomic_write(p: &Path, content: &str) -> std::io::Result<()> {
    if let Some(dir) = p.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(p).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_preview_commit() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.txt");
        std::fs::write(&a, "een\ntwee\n").unwrap();
        let t = FileTxn::new();
        t.write(&a, "een\n2\n".into());
        t.write(&d.path().join("sub/b.txt"), "nieuw\n".into());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "een\ntwee\n", "nog niets toegepast");
        assert_eq!(t.read(&a).unwrap(), "een\n2\n");
        let p = t.preview();
        assert!(p.contains("-twee") && p.contains("+2") && p.contains("+nieuw"), "{p}");
        assert_eq!(t.commit().unwrap(), 2);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "een\n2\n");
        assert!(d.path().join("sub/b.txt").exists());
    }

    #[test]
    fn rollback_leaves_disk_untouched() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.txt");
        std::fs::write(&a, "x").unwrap();
        let t = FileTxn::new();
        t.delete(&a);
        t.rollback();
        assert!(t.commit().is_ok());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "x");
    }
}
