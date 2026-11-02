//! Converters: Claude Skills (map met `SKILL.md`) ⇄ Cursor rules (`.mdc`) ⇄ AGENTS.md ⇄ Vortex.
//! Vortex-formaat = Claude-formaat + extra frontmatter (globs, always_apply, triggers, version).

use std::path::{Path, PathBuf};

use super::Skill;
use super::frontmatter::{Val, emit, split};

fn list_dir(dir: &Path, sub: &str) -> Vec<String> {
    let base = dir.join(sub);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            if e.path().is_file() {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// Importeer wat er op `path` staat: skill-map (Claude/Vortex), `.mdc` (Cursor), `AGENTS.md`,
/// of een losse `.md` met frontmatter.
pub fn import(path: &Path) -> Result<Vec<Skill>, String> {
    if path.is_dir() {
        let f = path.join("SKILL.md");
        if f.exists() {
            return import_skill_dir(path).map(|s| vec![s]);
        }
        let agents = path.join("AGENTS.md");
        if agents.exists() {
            return import_agents_md(&agents);
        }
        // Cursor-project: .cursor/rules/*.mdc
        let rules = path.join(".cursor").join("rules");
        let rules = if rules.is_dir() { rules } else { path.to_path_buf() };
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&rules) {
            for e in rd.flatten() {
                if e.path().extension().and_then(|x| x.to_str()) == Some("mdc") {
                    out.push(import_cursor(&e.path())?);
                }
            }
        }
        if out.is_empty() {
            return Err(format!("geen SKILL.md, AGENTS.md of .mdc in {}", path.display()));
        }
        return Ok(out);
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match () {
        _ if name.ends_with(".mdc") => import_cursor(path).map(|s| vec![s]),
        _ if name.eq_ignore_ascii_case("AGENTS.md") => import_agents_md(path),
        _ if name.eq_ignore_ascii_case("SKILL.md") => {
            import_skill_dir(path.parent().unwrap_or(Path::new("."))).map(|s| vec![s])
        }
        _ if name.ends_with(".md") => {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let (fm, body) = split(&text);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skill");
            Ok(vec![from_fields(&fm, body, stem, "vortex", None)])
        }
        _ => Err(format!("onbekend skillformaat: {}", path.display())),
    }
}

fn from_fields(
    fm: &std::collections::BTreeMap<String, Val>,
    body: String,
    fallback_name: &str,
    origin: &str,
    dir: Option<PathBuf>,
) -> Skill {
    let g = |k: &str| fm.get(k);
    let name = g("name").map(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or_else(|| fallback_name.to_string());
    let description = g("description").map(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or_else(|| {
        body.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .trim_start_matches('#')
            .trim()
            .chars()
            .take(200)
            .collect()
    });
    let known = [
        "name",
        "description",
        "globs",
        "alwaysApply",
        "always_apply",
        "triggers",
        "allowed-tools",
        "tools",
        "version",
        "origin",
    ];
    let extra = fm.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.as_str())).collect();
    let (scripts, resources) = match &dir {
        Some(d) => (list_dir(d, "scripts"), list_dir(d, "resources")),
        None => (vec![], vec![]),
    };
    Skill {
        name: Skill::slug(&name),
        description,
        body,
        globs: g("globs").map(|v| v.as_list()).unwrap_or_default(),
        always_apply: g("alwaysApply").or(g("always_apply")).is_some_and(|v| v.as_bool()),
        triggers: g("triggers").map(|v| v.as_list()).unwrap_or_default(),
        tools: g("allowed-tools").or(g("tools")).map(|v| v.as_list()).unwrap_or_default(),
        version: g("version").map(|v| v.as_str()).unwrap_or_else(|| "1".into()),
        origin: g("origin").map(|v| v.as_str()).unwrap_or_else(|| origin.to_string()),
        dir,
        scripts,
        resources,
        extra,
    }
}

/// Claude Skill of Vortex-skill (map met SKILL.md).
pub fn import_skill_dir(dir: &Path) -> Result<Skill, String> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).map_err(|e| e.to_string())?;
    let (fm, body) = split(&text);
    let fallback = dir.file_name().and_then(|n| n.to_str()).unwrap_or("skill");
    let origin = if fm.contains_key("origin") || fm.contains_key("globs") || fm.contains_key("triggers") {
        "vortex"
    } else {
        "claude"
    };
    Ok(from_fields(&fm, body, fallback, origin, Some(dir.to_path_buf())))
}

/// Cursor rule (`.mdc`): description, globs, alwaysApply; naam = bestandsnaam.
pub fn import_cursor(path: &Path) -> Result<Skill, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (fm, body) = split(&text);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("rule");
    Ok(from_fields(&fm, body, stem, "cursor", None))
}

/// AGENTS.md: elke `## `-sectie wordt een skill (zonder secties: één skill, altijd actief,
/// beperkt tot de map van het bestand via globs).
pub fn import_agents_md(path: &Path) -> Result<Vec<Skill>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let parent = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("project").to_string();
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if let Some((n, b)) = cur.take() {
                out.push((n, b));
            }
            cur = Some((h.trim().to_string(), Vec::new()));
        } else if let Some((_, b)) = cur.as_mut() {
            b.push(line.to_string());
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    if out.is_empty() {
        let (fm, body) = split(&text);
        let mut s = from_fields(&fm, body, &format!("agents-{parent}"), "agents", None);
        s.always_apply = true;
        return Ok(vec![s]);
    }
    Ok(out
        .into_iter()
        .map(|(heading, lines)| {
            let body = lines.join("\n").trim().to_string();
            // Beschrijving: een `> `-regel bovenaan (zo exporteren wij), anders de eerste zin.
            let (description, body) = match body.strip_prefix("> ") {
                Some(rest) => {
                    let (d, b) = rest.split_once('\n').unwrap_or((rest, ""));
                    (d.trim().to_string(), b.trim().to_string())
                }
                None => (body.split(['.', '\n']).next().unwrap_or("").trim().chars().take(200).collect(), body.clone()),
            };
            Skill {
                name: Skill::slug(&heading),
                description: if description.is_empty() { heading.clone() } else { description },
                body: format!("{body}\n"),
                origin: "agents".into(),
                version: "1".into(),
                ..Default::default()
            }
        })
        .collect())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

fn copy_assets(s: &Skill, out: &Path) -> std::io::Result<()> {
    if let Some(d) = &s.dir {
        for sub in ["scripts", "resources", "references", "assets"] {
            let from = d.join(sub);
            if from.is_dir() && from != out.join(sub) {
                copy_dir(&from, &out.join(sub))?;
            }
        }
    }
    Ok(())
}

/// Schrijf als Vortex-skill (`<dir>/<naam>/SKILL.md` + scripts/resources).
pub fn write_vortex(s: &Skill, root: &Path) -> std::io::Result<PathBuf> {
    let out = root.join(&s.name);
    std::fs::create_dir_all(&out)?;
    copy_assets(s, &out)?;
    let mut fields = vec![
        ("name", Val::Str(s.name.clone())),
        ("description", Val::Str(s.description.clone())),
        ("version", Val::Str(s.version.clone())),
        ("origin", Val::Str(if s.origin.is_empty() { "vortex".into() } else { s.origin.clone() })),
        ("globs", Val::List(s.globs.clone())),
        ("triggers", Val::List(s.triggers.clone())),
        ("allowed-tools", Val::List(s.tools.clone())),
    ];
    if s.always_apply {
        fields.push(("alwaysApply", Val::Bool(true)));
    }
    let extra: Vec<(String, Val)> = s.extra.iter().map(|(k, v)| (k.clone(), Val::Str(v.clone()))).collect();
    let mut all: Vec<(&str, Val)> = fields;
    for (k, v) in &extra {
        all.push((k.as_str(), v.clone()));
    }
    std::fs::write(out.join("SKILL.md"), format!("{}{}", emit(&all), s.body))?;
    Ok(out)
}

/// Exporteer als Claude Skill (`<out>/<naam>/SKILL.md`, alleen name/description + allowed-tools).
pub fn export_claude(s: &Skill, out_root: &Path) -> std::io::Result<PathBuf> {
    let out = out_root.join(&s.name);
    std::fs::create_dir_all(&out)?;
    copy_assets(s, &out)?;
    let desc: String = s.description.chars().take(1024).collect();
    let mut fields = vec![("name", Val::Str(s.name.clone())), ("description", Val::Str(desc))];
    if !s.tools.is_empty() {
        fields.push(("allowed-tools", Val::List(s.tools.clone())));
    }
    let mut body = s.body.clone();
    if !s.globs.is_empty() && !body.contains("Van toepassing op") {
        body = format!("Van toepassing op bestanden: {}\n\n{body}", s.globs.join(", "));
    }
    std::fs::write(out.join("SKILL.md"), format!("{}{}", emit(&fields), body))?;
    Ok(out)
}

/// Exporteer als Cursor rule (`<out>/<naam>.mdc`).
pub fn export_cursor(s: &Skill, out_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let p = out_dir.join(format!("{}.mdc", s.name));
    let fields = vec![
        ("description", Val::Str(s.description.clone())),
        ("globs", Val::Str(s.globs.join(","))),
        ("alwaysApply", Val::Bool(s.always_apply)),
    ];
    let mut body = s.body.clone();
    if !s.scripts.is_empty() {
        body.push_str(&format!("\n\nScripts (in de Vortex-skillmap): {}\n", s.scripts.join(", ")));
    }
    std::fs::write(&p, format!("{}{}", emit(&fields), body))?;
    Ok(p)
}

/// Exporteer een set skills als één AGENTS.md.
pub fn export_agents_md(skills: &[Skill], out_file: &Path) -> std::io::Result<PathBuf> {
    let mut text =
        String::from("# AGENTS.md\n\nInstructies voor AI-agents in dit project (geëxporteerd uit Vortex).\n");
    for s in skills {
        text.push_str(&format!("\n## {}\n\n> {}\n\n{}\n", s.name, s.description, s.body.trim()));
    }
    if let Some(d) = out_file.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(out_file, text)?;
    Ok(out_file.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_cursor_agents_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        // Claude Skill met script.
        let cs = d.path().join("in/pdf-tools");
        std::fs::create_dir_all(cs.join("scripts")).unwrap();
        std::fs::write(cs.join("SKILL.md"), "---\nname: pdf-tools\ndescription: Splits en voeg PDF's samen\nallowed-tools: [Read, Bash]\n---\n# PDF\nGebruik scripts/split.py.\n").unwrap();
        std::fs::write(cs.join("scripts/split.py"), "print('ok')").unwrap();
        let s = import(&cs).unwrap().remove(0);
        assert_eq!(s.origin, "claude");
        assert_eq!(s.tools, vec!["Read", "Bash"]);
        assert_eq!(s.scripts, vec!["split.py"]);

        // → Cursor → terug.
        let mut s2 = s.clone();
        s2.globs = vec!["**/*.pdf".into()];
        let mdc = export_cursor(&s2, &d.path().join("cursor")).unwrap();
        let back = import(&mdc).unwrap().remove(0);
        assert_eq!(back.name, "pdf-tools");
        assert_eq!(back.description, s.description);
        assert_eq!(back.globs, vec!["**/*.pdf"]);
        assert!(back.body.starts_with("# PDF"));

        // → Vortex (met script) → Claude.
        let vx = write_vortex(&s2, &d.path().join("vortex")).unwrap();
        assert!(vx.join("scripts/split.py").exists());
        let v = import(&vx).unwrap().remove(0);
        assert_eq!(v.origin, "claude", "herkomst blijft behouden");
        assert_eq!(v.globs, vec!["**/*.pdf"]);
        let cl = export_claude(&v, &d.path().join("claude")).unwrap();
        let c = import(&cl).unwrap().remove(0);
        assert_eq!(c.name, "pdf-tools");
        assert!(c.body.contains("**/*.pdf"), "globs als tekst in Claude-skill");

        // → AGENTS.md → terug.
        let am = export_agents_md(std::slice::from_ref(&v), &d.path().join("agents/AGENTS.md")).unwrap();
        let a = import(&am).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].name, "pdf-tools");
        assert_eq!(a[0].description, v.description);
        assert!(a[0].body.contains("Gebruik scripts/split.py."));
    }

    #[test]
    fn cursor_project_dir() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().join(".cursor/rules");
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(r.join("react.mdc"), "---\ndescription: React-conventies\nglobs: src/**/*.tsx\nalwaysApply: false\n---\nGebruik function components.\n").unwrap();
        std::fs::write(r.join("altijd.mdc"), "---\ndescription: Altijd\nalwaysApply: true\n---\nSchrijf Nederlands.\n")
            .unwrap();
        let v = import(d.path()).unwrap();
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|s| s.name == "altijd" && s.always_apply));
    }
}
