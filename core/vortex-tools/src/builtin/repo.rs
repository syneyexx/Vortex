//! Git en repo-niveau codebegrip (W17.14): `git` (status, diff, log, show, blame, branch, add,
//! commit — binnen de toegestane map), `repo_map` (bestandsboom + talen + top-level symbolen
//! per bestand: functies, types, klassen) en `repo_search` (regex door de code, met regelnummers).
// VX:W17.14

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::{s, u};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::tool::{Tool, ToolCtx, ToolOutput};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Git::new()), Arc::new(RepoMap::new()), Arc::new(RepoSearch::new())]
}

const SKIP_DIRS: &[&str] =
    &[".git", "node_modules", "target", "dist", "build", ".venv", "venv", "__pycache__", ".next", ".idea", ".vscode"];

fn lang(p: &Path) -> Option<&'static str> {
    Some(match p.extension()?.to_str()?.to_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "lua" => "lua",
        "sql" => "sql",
        "md" => "markdown",
        "toml" | "yaml" | "yml" | "json" => "config",
        _ => return None,
    })
}

/// Bestanden van de repo: via `git ls-files` als het een git-repo is (respecteert .gitignore),
/// anders zelf lopen (met de gebruikelijke mappen overgeslagen).
async fn files(root: &Path, max: usize) -> Vec<PathBuf> {
    if let Ok(o) = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-co", "--exclude-standard"])
        .output()
        .await
        && o.status.success()
    {
        return String::from_utf8_lossy(&o.stdout).lines().take(max).map(|l| root.join(l)).collect();
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(p);
                }
            } else if out.len() < max {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Top-level symbolen (eenvoudige, taalbewuste patronen; geen volledige parser).
pub fn symbols(lang: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        let take = |prefixes: &[&str]| {
            prefixes
                .iter()
                .find_map(|p| t.strip_prefix(p))
                .map(|r| r.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("").to_string())
        };
        let sym = match lang {
            "rust" if indent == 0 => take(&[
                "pub fn ",
                "fn ",
                "pub async fn ",
                "async fn ",
                "pub struct ",
                "struct ",
                "pub enum ",
                "enum ",
                "pub trait ",
                "trait ",
                "impl ",
                "pub mod ",
                "mod ",
                "pub(crate) fn ",
                "pub type ",
                "type ",
            ])
            .map(|n| {
                format!(
                    "{} {n}",
                    t.split_whitespace()
                        .find(|w| ["fn", "struct", "enum", "trait", "impl", "mod", "type"].contains(w))
                        .unwrap_or("")
                )
            }),
            "python" if indent == 0 => take(&["def ", "async def ", "class "])
                .map(|n| format!("{} {n}", if t.starts_with("class") { "class" } else { "def" })),
            "typescript" | "javascript" if indent == 0 => take(&[
                "export function ",
                "function ",
                "export async function ",
                "export class ",
                "class ",
                "export interface ",
                "interface ",
                "export type ",
                "export const ",
                "const ",
            ])
            .map(|n| {
                format!(
                    "{} {n}",
                    t.split_whitespace()
                        .find(|w| ["function", "class", "interface", "type", "const"].contains(w))
                        .unwrap_or("")
                )
            }),
            "go" if indent == 0 => take(&["func ", "type "])
                .map(|n| format!("{} {n}", if t.starts_with("func") { "func" } else { "type" })),
            "java" | "csharp" | "kotlin" if indent <= 4 => {
                (t.contains(" class ") || t.starts_with("class ") || t.contains(" interface "))
                    .then(|| t.chars().take(80).collect())
            }
            _ => None,
        };
        if let Some(s) = sym.filter(|s| s.split_whitespace().nth(1).is_some_and(|n| !n.is_empty())) {
            out.push(s);
        }
    }
    out
}

#[derive(Debug)]
pub struct Git {
    c: ToolContract,
}
impl Git {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "git",
                "Git in een repo binnen de werkmap: status, diff, log, show, blame, branch, add, commit (args als lijst, bv. [\"log\", \"-5\", \"--oneline\"]).",
                Effect::Local,
                json!({"type": "object", "required": ["args"], "properties": {
                    "repo": {"type": "string", "description": "pad van de repo (standaard werkmap)"},
                    "args": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                }}),
            )
            .resources(vec![ResourceUse::Cpu])
            .timeout(120_000),
        }
    }
}
impl Default for Git {
    fn default() -> Self {
        Self::new()
    }
}
const GIT_ALLOWED: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "blame",
    "branch",
    "add",
    "commit",
    "rev-parse",
    "ls-files",
    "shortlog",
    "describe",
    "stash",
    "checkout",
    "switch",
    "restore",
    "tag",
    "grep",
];

#[async_trait::async_trait]
impl Tool for Git {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        if !ctx.caps.processes {
            return Err(ToolError::new(ToolErrorKind::Capability, "deze taak mag geen processen starten"));
        }
        let repo = ctx.caps.resolve(if s(&i, "repo").is_empty() { "." } else { s(&i, "repo") })?;
        let args: Vec<String> =
            i["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
        let sub = args.first().map(String::as_str).unwrap_or("");
        if !GIT_ALLOWED.contains(&sub) {
            return Err(ToolError::invalid(format!("git {sub} niet ondersteund"))
                .with_hint(format!("gebruik een van: {}", GIT_ALLOWED.join(", "))));
        }
        // Netwerk (push/pull/fetch) en -C/--git-dir om de map te ontlopen: niet via deze tool.
        if args.iter().any(|a| a == "-C" || a.starts_with("--git-dir") || a.starts_with("--work-tree")) {
            return Err(ToolError::invalid("-C/--git-dir/--work-tree niet toegestaan; gebruik `repo`"));
        }
        let o = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "core.pager=cat", "-c", "color.ui=false"])
            .args(&args)
            .output()
            .await?;
        let out = String::from_utf8_lossy(&o.stdout).to_string();
        let err = String::from_utf8_lossy(&o.stderr).to_string();
        if !o.status.success() {
            return Err(ToolError::failed(format!("git {sub}: {}", err.trim()))
                .with_hint("controleer de argumenten of de repo-staat"));
        }
        let text = if out.chars().count() > 20_000 {
            format!("{}…\n(ingekort)", out.chars().take(20_000).collect::<String>())
        } else {
            out.clone()
        };
        Ok(ToolOutput {
            data: json!({"stdout": out, "stderr": err}),
            text: if text.is_empty() { format!("git {sub}: ok") } else { text },
            preview: None,
        })
    }
}

#[derive(Debug)]
pub struct RepoMap {
    c: ToolContract,
}
impl RepoMap {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "repo_map",
                "Overzicht van een codebase: bestanden per map, talen, regels en top-level symbolen (functies, types, klassen) — eerst dit, dan gericht lezen.",
                Effect::Read,
                json!({"type": "object", "properties": {
                    "path": {"type": "string"},
                    "max_files": {"type": "integer", "minimum": 1, "maximum": 20000},
                    "symbols": {"type": "boolean"}
                }}),
            )
            .timeout(120_000),
        }
    }
}
impl Default for RepoMap {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for RepoMap {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let root = ctx.caps.resolve(if s(&i, "path").is_empty() { "." } else { s(&i, "path") })?;
        let with_syms = i.get("symbols").and_then(|b| b.as_bool()).unwrap_or(true);
        let fs = files(&root, u(&i, "max_files").unwrap_or(3000) as usize).await;
        let mut langs: std::collections::BTreeMap<&str, (usize, usize)> = std::collections::BTreeMap::new();
        let mut entries = Vec::new();
        let mut text = String::new();
        for f in &fs {
            let Some(l) = lang(f) else { continue };
            let content = std::fs::read_to_string(f).unwrap_or_default();
            let lines = content.lines().count();
            let e = langs.entry(l).or_default();
            e.0 += 1;
            e.1 += lines;
            let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
            let syms = if with_syms && l != "config" && l != "markdown" { symbols(l, &content) } else { vec![] };
            text.push_str(&format!("{rel} ({l}, {lines} r.)"));
            if !syms.is_empty() {
                text.push_str(&format!(": {}", syms.iter().take(25).cloned().collect::<Vec<_>>().join(", ")));
            }
            text.push('\n');
            entries.push(json!({"path": rel, "lang": l, "lines": lines, "symbols": syms}));
        }
        let summary =
            langs.iter().map(|(l, (n, r))| format!("{l}: {n} bestanden, {r} regels")).collect::<Vec<_>>().join("; ");
        Ok(ToolOutput {
            data: json!({"root": root, "languages": langs.iter().map(|(l, (n, r))| json!({"lang": l, "files": n, "lines": r})).collect::<Vec<_>>(), "files": entries}),
            text: format!("{summary}\n\n{text}"),
            preview: None,
        })
    }
}

#[derive(Debug)]
pub struct RepoSearch {
    c: ToolContract,
}
impl RepoSearch {
    pub fn new() -> Self {
        Self {
            c: ToolContract::new(
                "repo_search",
                "Zoek met een regex door de code (bestand:regel: tekst), optioneel beperkt tot een taal of padfragment.",
                Effect::Read,
                json!({"type": "object", "required": ["pattern"], "properties": {
                    "pattern": {"type": "string", "minLength": 1},
                    "path": {"type": "string"},
                    "lang": {"type": "string"},
                    "max_hits": {"type": "integer", "minimum": 1, "maximum": 2000}
                }}),
            )
            .timeout(120_000),
        }
    }
}
impl Default for RepoSearch {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl Tool for RepoSearch {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let root = ctx.caps.resolve(if s(&i, "path").is_empty() { "." } else { s(&i, "path") })?;
        let re = regex_lite::Regex::new(s(&i, "pattern")).map_err(|e| ToolError::invalid(format!("regex: {e}")))?;
        let want = s(&i, "lang");
        let max = u(&i, "max_hits").unwrap_or(200) as usize;
        let mut hits = Vec::new();
        'outer: for f in files(&root, 20_000).await {
            let Some(l) = lang(&f) else { continue };
            if !want.is_empty() && l != want {
                continue;
            }
            let Ok(c) = std::fs::read_to_string(&f) else { continue };
            for (n, line) in c.lines().enumerate() {
                if re.is_match(line) {
                    hits.push(json!({"file": f.strip_prefix(&root).unwrap_or(&f).display().to_string(), "line": n + 1, "text": line.trim().chars().take(200).collect::<String>()}));
                    if hits.len() >= max {
                        break 'outer;
                    }
                }
            }
        }
        let text = hits
            .iter()
            .map(|h| {
                format!("{}:{}: {}", h["file"].as_str().unwrap_or(""), h["line"], h["text"].as_str().unwrap_or(""))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput {
            data: json!({"hits": hits}),
            text: if text.is_empty() { "geen treffers".into() } else { text },
            preview: None,
        })
    }
}
