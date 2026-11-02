//! Capability handles per run (W08.12): technisch bereik van een taak (mappen, netwerk,
//! schrijven, processen). Standaard: alles (geen remmen tot de ethics-laag, W19). Een run kan
//! zich beperken tot bv. één projectmap; tools controleren paden hiertegen.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ToolError, ToolErrorKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityHandle {
    /// Toegestane wortels (leeg = overal).
    pub roots: Vec<PathBuf>,
    /// Werkmap voor relatieve paden.
    pub cwd: PathBuf,
    pub net: bool,
    pub write: bool,
    pub processes: bool,
    /// Toegestane tools (leeg = alle).
    pub tools: Vec<String>,
}

impl Default for CapabilityHandle {
    fn default() -> Self {
        Self {
            roots: vec![],
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            net: true,
            write: true,
            processes: true,
            tools: vec![],
        }
    }
}

/// Pad normaliseren zonder dat het hoeft te bestaan (`..` en `.` wegwerken).
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl CapabilityHandle {
    /// Beperk tot één map (die wordt ook de werkmap).
    pub fn scoped(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { roots: vec![root.clone()], cwd: root, ..Default::default() }
    }

    /// Absoluut, genormaliseerd pad binnen de handle (symlinks worden gevolgd als ze bestaan).
    pub fn resolve(&self, p: &str) -> Result<PathBuf, ToolError> {
        let raw = Path::new(p);
        let abs = if raw.is_absolute() { raw.to_path_buf() } else { self.cwd.join(raw) };
        let mut norm = normalize(&abs);
        // Bestaand pad (of bestaande ouder): canonicaliseren zodat symlinks niet ontsnappen.
        if let Ok(c) = std::fs::canonicalize(&norm) {
            norm = c;
        } else if let (Some(parent), Some(name)) = (norm.parent(), norm.file_name())
            && let Ok(c) = std::fs::canonicalize(parent)
        {
            norm = c.join(name);
        }
        if self.roots.is_empty() {
            return Ok(norm);
        }
        let inside = self.roots.iter().any(|r| {
            let r = std::fs::canonicalize(r).unwrap_or_else(|_| normalize(r));
            norm.starts_with(&r)
        });
        if inside {
            Ok(norm)
        } else {
            Err(ToolError::new(
                ToolErrorKind::Capability,
                format!("{} valt buiten het bereik van deze taak", norm.display()),
            )
            .with_hint(format!(
                "toegestaan: {}",
                self.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")
            )))
        }
    }

    pub fn allows_tool(&self, name: &str) -> bool {
        self.tools.is_empty()
            || self.tools.iter().any(|t| t == name || (t.ends_with('*') && name.starts_with(&t[..t.len() - 1])))
    }

    pub fn check_effect(&self, effect: crate::contract::Effect, needs_net: bool) -> Result<(), ToolError> {
        if effect != crate::contract::Effect::Read && !self.write {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag niets wijzigen"));
        }
        if needs_net && !self.net {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak heeft geen netwerk"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_handle_blocks_escape() {
        let d = tempfile::tempdir().unwrap();
        let h = CapabilityHandle::scoped(d.path());
        assert!(h.resolve("a/b.txt").is_ok());
        assert!(h.resolve("../x").is_err());
        assert!(h.resolve("a/../../x").is_err());
        assert!(CapabilityHandle::default().resolve("/etc/hosts").is_ok());
    }

    #[test]
    fn tool_globs() {
        let h = CapabilityHandle { tools: vec!["read_*".into(), "web_search".into()], ..Default::default() };
        assert!(h.allows_tool("read_file"));
        assert!(h.allows_tool("web_search"));
        assert!(!h.allows_tool("write_file"));
    }
}
