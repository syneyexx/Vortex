//! Bestanden: lezen, schrijven, bewerken, map tonen, zoeken in inhoud, bestanden vinden.
//! Schrijven loopt via de transactie van de executor (preview + atomair toepassen).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::{b, s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::tool::{Tool, ToolCtx, ToolOutput};

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "__pycache__", ".venv", "dist", ".next", ".cache"];

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadFile::new()),
        Arc::new(WriteFile::new()),
        Arc::new(EditFile::new()),
        Arc::new(DeleteFile::new()),
        Arc::new(ListDir::new()),
        Arc::new(SearchFiles::new()),
        Arc::new(FindFiles::new()),
    ]
}

fn read_text(ctx: &ToolCtx, p: &Path) -> Result<String, ToolError> {
    let r = match &ctx.txn {
        Some(t) => t.read(p),
        None => std::fs::read_to_string(p),
    };
    r.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            ToolError::new(ToolErrorKind::Precondition, format!("{} bestaat niet", p.display()))
        }
        std::io::ErrorKind::InvalidData => ToolError::failed(format!("{} is geen tekstbestand (UTF-8)", p.display())),
        _ => e.into(),
    })
}

fn stage_write(ctx: &ToolCtx, p: &Path, content: String) -> Result<(), ToolError> {
    match &ctx.txn {
        Some(t) => {
            t.write(p, content);
            Ok(())
        }
        None => crate::txn::atomic_write(p, &content).map_err(Into::into),
    }
}

macro_rules! contract_tool {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name {
            c: ToolContract,
        }
    };
}

contract_tool!(ReadFile);
impl ReadFile {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "read_file",
                "Lees een tekstbestand (met regelnummers); offset/limit voor grote bestanden.",
                Effect::Read,
                json!({"type": "object", "required": ["path"], "properties": {
                    "path": {"type": "string", "minLength": 1},
                    "offset": {"type": "integer", "minimum": 1, "description": "eerste regel (1-based)"},
                    "limit": {"type": "integer", "minimum": 1, "default": 400}
                }}),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(10_000),
        }
    }
}
impl Default for ReadFile {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for ReadFile {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let p = ctx.caps.resolve(s(&i, "path"))?;
        if p.is_dir() {
            return Err(ToolError::invalid(format!("{} is een map", p.display())).with_hint("gebruik list_dir"));
        }
        let text = read_text(ctx, &p)?;
        let lines: Vec<&str> = text.lines().collect();
        let off = u(&i, "offset").unwrap_or(1).max(1) as usize;
        let lim = u(&i, "limit").unwrap_or(400) as usize;
        let end = (off - 1 + lim).min(lines.len());
        let body: String =
            lines.iter().enumerate().skip(off - 1).take(lim).map(|(n, l)| format!("{:>5}  {l}\n", n + 1)).collect();
        let head = if end < lines.len() || off > 1 {
            format!("{} — regels {off}-{end} van {}\n", p.display(), lines.len())
        } else {
            String::new()
        };
        Ok(ToolOutput {
            data: json!({"path": p.display().to_string(), "content": lines[(off - 1).min(lines.len())..end].join("\n"), "total_lines": lines.len()}),
            text: format!("{head}{body}"),
            preview: None,
        })
    }
}

contract_tool!(WriteFile);
impl WriteFile {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "write_file",
                "Schrijf een bestand (nieuw of volledig vervangen); maakt mappen aan. Toont een diff.",
                Effect::Local,
                json!({"type": "object", "required": ["path", "content"], "properties": {
                    "path": {"type": "string", "minLength": 1},
                    "content": {"type": "string"}
                }}),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(10_000),
        }
    }
}
impl Default for WriteFile {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for WriteFile {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let p = ctx.caps.resolve(s(&i, "path"))?;
        let content = s(&i, "content").to_string();
        let old = read_text(ctx, &p).ok();
        let diff = crate::diff::unified(&p.display().to_string(), old.as_deref().unwrap_or(""), &content, 3);
        let n = content.lines().count();
        stage_write(ctx, &p, content)?;
        Ok(ToolOutput::json(json!({"path": p.display().to_string(), "lines": n, "created": old.is_none()}))
            .with_text(format!(
                "{} {} ({n} regels)",
                if old.is_none() { "Aangemaakt" } else { "Overschreven" },
                p.display()
            ))
            .with_preview(diff))
    }
}

contract_tool!(EditFile);
impl EditFile {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "edit_file",
                "Vervang exact `old` door `new` in een bestand (moet precies één keer voorkomen, tenzij replace_all).",
                Effect::Local,
                json!({"type": "object", "required": ["path", "old", "new"], "properties": {
                    "path": {"type": "string", "minLength": 1},
                    "old": {"type": "string", "minLength": 1},
                    "new": {"type": "string"},
                    "replace_all": {"type": "boolean", "default": false}
                }}),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(10_000),
        }
    }
}
impl Default for EditFile {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for EditFile {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let p = ctx.caps.resolve(s(&i, "path"))?;
        let text = read_text(ctx, &p)?;
        let (old, new) = (s(&i, "old"), s(&i, "new"));
        let n = text.matches(old).count();
        if n == 0 {
            // Herstelhint: de regel met de meeste overlap.
            let first = old.lines().next().unwrap_or(old).trim();
            let near = text
                .lines()
                .enumerate()
                .find(|(_, l)| !first.is_empty() && l.contains(first))
                .map(|(k, l)| format!("regel {}: {l}", k + 1));
            let mut e = ToolError::new(ToolErrorKind::Precondition, "tekst `old` niet gevonden");
            e.hint = Some(near.map_or("lees het bestand eerst met read_file en kopieer de tekst exact".into(), |x| {
                format!("lijkt op {x} — let op spaties/inspringing")
            }));
            return Err(e);
        }
        if n > 1 && !b(&i, "replace_all") {
            return Err(ToolError::new(ToolErrorKind::Precondition, format!("`old` komt {n}× voor"))
                .with_hint("maak `old` langer (meer context) of zet replace_all"));
        }
        let updated = if b(&i, "replace_all") { text.replace(old, new) } else { text.replacen(old, new, 1) };
        let diff = crate::diff::unified(&p.display().to_string(), &text, &updated, 3);
        stage_write(ctx, &p, updated)?;
        Ok(ToolOutput::json(
            json!({"path": p.display().to_string(), "replacements": if b(&i, "replace_all") { n } else { 1 }}),
        )
        .with_text(format!("Bewerkt: {} ({}× vervangen)", p.display(), if b(&i, "replace_all") { n } else { 1 }))
        .with_preview(diff))
    }
}

contract_tool!(DeleteFile);
impl DeleteFile {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "delete_file",
                "Verwijder een bestand.",
                Effect::Local,
                json!({"type": "object", "required": ["path"], "properties": {"path": {"type": "string", "minLength": 1}}}),
            )
            .resources(vec![ResourceUse::Disk]),
        }
    }
}
impl Default for DeleteFile {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for DeleteFile {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let p = ctx.caps.resolve(s(&i, "path"))?;
        let old = read_text(ctx, &p)?;
        let diff = crate::diff::unified(&p.display().to_string(), &old, "", 0);
        match &ctx.txn {
            Some(t) => t.delete(&p),
            None => std::fs::remove_file(&p)?,
        }
        Ok(ToolOutput::json(json!({"path": p.display().to_string()}))
            .with_text(format!("Verwijderd: {}", p.display()))
            .with_preview(diff))
    }
}

contract_tool!(ListDir);
impl ListDir {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "list_dir",
                "Toon de inhoud van een map (bestanden met grootte), optioneel een paar niveaus diep.",
                Effect::Read,
                json!({"type": "object", "properties": {
                    "path": {"type": "string", "default": "."},
                    "depth": {"type": "integer", "minimum": 1, "maximum": 6, "default": 1}
                }}),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(15_000),
        }
    }
}
impl Default for ListDir {
    fn default() -> Self {
        Self::new()
    }
}
fn walk_dir(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<String>, limit: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= limit {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let indent = "  ".repeat(depth);
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            out.push(format!("{indent}{name}/"));
            if depth + 1 < max_depth && !SKIP_DIRS.contains(&name.as_str()) {
                walk_dir(&e.path(), depth + 1, max_depth, out, limit);
            }
        } else {
            out.push(format!("{indent}{name}  ({})", human(md.len())));
        }
    }
}
fn human(n: u64) -> String {
    match n {
        n if n >= 1 << 30 => format!("{:.1} GB", n as f64 / (1u64 << 30) as f64),
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KB", n as f64 / 1024.0),
        n => format!("{n} B"),
    }
}
#[async_trait::async_trait]
impl Tool for ListDir {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = if s(&i, "path").is_empty() { "." } else { s(&i, "path") };
        let p = ctx.caps.resolve(path)?;
        if !p.is_dir() {
            return Err(ToolError::new(ToolErrorKind::Precondition, format!("{} is geen map", p.display())));
        }
        let depth = u(&i, "depth").unwrap_or(1) as usize;
        let (p2, out) = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            walk_dir(&p, 0, depth, &mut out, 2000);
            (p, out)
        })
        .await
        .map_err(|e| ToolError::failed(e.to_string()))?;
        Ok(ToolOutput::json(json!({"path": p2.display().to_string(), "entries": out})).with_text(format!(
            "{}\n{}",
            p2.display(),
            out.join("\n")
        )))
    }
}

/// Bestanden onder `root` (met skip-lijst), relatief pad met `/`.
pub(crate) fn files_under(root: &Path, limit: usize) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut es: Vec<_> = rd.flatten().collect();
        es.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for e in es {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
            } else {
                let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push((p, rel));
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}

contract_tool!(SearchFiles);
impl SearchFiles {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "search_files",
                "Zoek tekst in bestanden onder een map (hoofdletterongevoelig); glob om te beperken (bv. **/*.rs).",
                Effect::Read,
                json!({"type": "object", "required": ["query"], "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "path": {"type": "string", "default": "."},
                    "glob": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50}
                }}),
            )
            .resources(vec![ResourceUse::Disk, ResourceUse::Cpu])
            .timeout(30_000),
        }
    }
}
impl Default for SearchFiles {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for SearchFiles {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let root = ctx.caps.resolve(if s(&i, "path").is_empty() { "." } else { s(&i, "path") })?;
        let q = s(&i, "query").to_lowercase();
        let glob = i.get("glob").and_then(|g| g.as_str()).map(String::from);
        let max = u(&i, "max_results").unwrap_or(50) as usize;
        let cancel = ctx.cancel.clone();
        let hits = tokio::task::spawn_blocking(move || {
            let mut hits = Vec::new();
            for (p, rel) in files_under(&root, 50_000) {
                if cancel.is_cancelled() || hits.len() >= max {
                    break;
                }
                if let Some(g) = &glob
                    && !crate::glob::matches(g, &rel)
                {
                    continue;
                }
                let Ok(meta) = std::fs::metadata(&p) else { continue };
                if meta.len() > 5 << 20 {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                for (n, l) in text.lines().enumerate() {
                    if l.to_lowercase().contains(&q) {
                        hits.push(
                            json!({"file": rel, "line": n + 1, "text": l.trim().chars().take(240).collect::<String>()}),
                        );
                        if hits.len() >= max {
                            break;
                        }
                    }
                }
            }
            hits
        })
        .await
        .map_err(|e| ToolError::failed(e.to_string()))?;
        let text = if hits.is_empty() {
            "Geen treffers.".to_string()
        } else {
            hits.iter()
                .map(|h| format!("{}:{}: {}", s(h, "file"), h["line"], s(h, "text")))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::json(json!({"hits": hits})).with_text(text))
    }
}

contract_tool!(FindFiles);
impl FindFiles {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "find_files",
                "Vind bestanden op naam/glob in lokale mappen (bv. **/*.pdf, *rapport*).",
                Effect::Read,
                json!({"type": "object", "required": ["pattern"], "properties": {
                    "pattern": {"type": "string", "minLength": 1},
                    "path": {"type": "string", "default": "."},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 2000, "default": 200}
                }}),
            )
            .resources(vec![ResourceUse::Disk])
            .timeout(30_000),
        }
    }
}
impl Default for FindFiles {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for FindFiles {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let root = ctx.caps.resolve(if s(&i, "path").is_empty() { "." } else { s(&i, "path") })?;
        let pat = s(&i, "pattern").to_string();
        let max = u(&i, "max_results").unwrap_or(200) as usize;
        let found = tokio::task::spawn_blocking(move || {
            files_under(&root, 200_000)
                .into_iter()
                .filter(|(_, rel)| crate::glob::matches(&pat, rel))
                .take(max)
                .map(|(_, rel)| rel)
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| ToolError::failed(e.to_string()))?;
        let text = if found.is_empty() { "Niets gevonden.".into() } else { found.join("\n") };
        Ok(ToolOutput::json(json!({"files": found})).with_text(text))
    }
}
