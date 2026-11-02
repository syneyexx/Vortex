//! Ingest (W10.01–W10.05, W10.14): bestanden/mappen → Python-worker (parsen, JSONL op de koude
//! schijf) → geheugen (structuur-bewust chunken, contextbeschrijving, embeddings, claims/graph,
//! dedup, conflictcheck — de schrijfpijplijn van W05) met exacte locator per sectie →
//! hiërarchische samenvattingen (sectie → hoofdstuk → boek). Incrementeel op content-hash.
//! Zonder worker: eenvoudige Rust-parser voor Markdown/tekst/HTML/code.
// VX:W10.01

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use vortex_core::RunSpec;
use vortex_memory::{MemoryKind, NewMemory, WriteOutcome};
use vortex_proto::common::v1::TraceContext;
use vortex_proto::worker::v1::worker_service_client::WorkerServiceClient;
use vortex_proto::worker::v1::{IngestRequest, ingest_update::Update};
use vortex_types::{Event, EventKind, MemoryId, Priority, Provenance, RunId, RunKind, SourceKind};

use crate::chat::ChatEngine;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub level: u32,
    pub text: String,
    pub locator: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Doc {
    pub path: String,
    pub content_hash: String,
    pub metadata: serde_json::Map<String, serde_json::Value>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IngestReport {
    pub docs: usize,
    pub sections: usize,
    pub skipped: usize,
    pub summaries: usize,
    pub errors: Vec<String>,
    pub via_worker: bool,
}

/// JSONL van de worker lezen.
pub fn read_jsonl(path: &Path) -> Result<Doc, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc = Doc::default();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        match v["type"].as_str() {
            Some("doc") => {
                doc.path = v["path"].as_str().unwrap_or("").into();
                doc.content_hash = v["content_hash"].as_str().unwrap_or("").into();
                doc.metadata = v["metadata"].as_object().cloned().unwrap_or_default();
            }
            Some("section") => doc.sections.push(serde_json::from_value(v).map_err(|e| e.to_string())?),
            _ => {}
        }
    }
    Ok(doc)
}

/// Eenvoudige Rust-parser (zonder worker): Markdown-koppen, HTML, code en tekst met regelbereik.
pub fn parse_native(path: &Path) -> Result<Doc, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ["pdf", "epub", "docx"].contains(&ext.as_str()) {
        return Err(format!("{} vereist de Python-worker (just worker)", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let mut sections = Vec::new();
    let mut title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("document").to_string();
    match ext.as_str() {
        "md" | "markdown" => {
            let mut cur = Section { locator: "regel 1".into(), ..Default::default() };
            let mut code = false;
            for (n, line) in text.lines().enumerate() {
                if line.starts_with("```") {
                    code = !code;
                }
                let head = (!code).then(|| line.trim_start()).filter(|l| l.starts_with('#')).and_then(|l| {
                    let lvl = l.chars().take_while(|c| *c == '#').count();
                    l[lvl..].strip_prefix(' ').map(|t| (lvl, t.trim().to_string()))
                });
                match head {
                    Some((lvl, t)) if lvl <= 6 => {
                        if !cur.text.trim().is_empty() {
                            sections.push(std::mem::take(&mut cur));
                        }
                        if lvl == 1 && sections.is_empty() {
                            title = t.clone();
                        }
                        cur = Section {
                            title: t.clone(),
                            level: lvl as u32,
                            text: String::new(),
                            locator: format!("regel {} > {t}", n + 1),
                        };
                    }
                    _ => {
                        cur.text.push_str(line);
                        cur.text.push('\n');
                    }
                }
            }
            if !cur.text.trim().is_empty() {
                sections.push(cur);
            }
        }
        "html" | "htm" => {
            let (t, body) = vortex_tools::builtin::web::html_to_text(&text);
            if !t.is_empty() {
                title = t.clone();
            }
            sections.push(Section { title: t, level: 1, text: body, locator: "pagina".into() });
        }
        _ => {
            let lines: Vec<&str> = text.lines().collect();
            for (i, chunk) in lines.chunks(60).enumerate() {
                let body = chunk.join("\n");
                if !body.trim().is_empty() {
                    sections.push(Section {
                        title: String::new(),
                        level: 0,
                        text: body,
                        locator: format!("regels {}-{}", i * 60 + 1, i * 60 + chunk.len()),
                    });
                }
            }
        }
    }
    let mut metadata = serde_json::Map::new();
    metadata.insert("title".into(), json!(title));
    metadata.insert("format".into(), json!(ext));
    Ok(Doc {
        path: std::fs::canonicalize(path).unwrap_or(path.to_path_buf()).display().to_string(),
        content_hash: format!("{:016x}", vortex_tools::stable_hash(&bytes)),
        metadata,
        sections,
    })
}

fn collect_native(paths: &[String]) -> Vec<PathBuf> {
    const EXT: &[&str] = &[
        "md", "markdown", "txt", "html", "htm", "rs", "py", "ts", "tsx", "js", "toml", "json", "csv", "pdf", "epub",
        "docx",
    ];
    let mut out = Vec::new();
    for p in paths.iter().map(PathBuf::from) {
        if p.is_dir() {
            let mut stack = vec![p];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let q = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if q.is_dir() {
                        if ![".git", "node_modules", "target", ".venv"].contains(&name.as_str()) {
                            stack.push(q);
                        }
                    } else if q
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| EXT.contains(&x.to_lowercase().as_str()))
                    {
                        out.push(q);
                    }
                }
            }
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

impl ChatEngine {
    /// Worker-adres uit `config/paths.toml` (`worker_address`), standaard 127.0.0.1:50061.
    fn worker_address(&self) -> String {
        self.kernel
            .config()
            .current()
            .get::<String>("paths.worker_address")
            .ok()
            .flatten()
            .unwrap_or_else(|| "127.0.0.1:50061".into())
    }

    /// Documenten via de worker; `None` als de worker niet bereikbaar is.
    async fn worker_docs(
        &self,
        paths: &[String],
        run: RunId,
        trace: vortex_types::TraceId,
        progress: &(dyn Fn(f64, &str) + Sync),
    ) -> Option<(Vec<Doc>, Vec<String>)> {
        let ep = tonic::transport::Endpoint::from_shared(format!("http://{}", self.worker_address()))
            .ok()?
            .connect_timeout(Duration::from_secs(2));
        let ch = ep.connect().await.ok()?;
        let mut client = WorkerServiceClient::new(ch);
        let req = IngestRequest {
            trace: Some(TraceContext { trace_id: trace.to_string(), span_id: String::new(), run_id: run.to_string() }),
            paths: paths.to_vec(),
            workspace_id: String::new(),
        };
        let mut stream = client.ingest(req).await.ok()?.into_inner();
        let (mut docs, mut errors) = (Vec::new(), Vec::new());
        loop {
            match stream.message().await {
                Ok(Some(u)) => match u.update {
                    Some(Update::Progress(p)) => progress(p.fraction * 0.5, &format!("{} {}", p.stage, p.message)),
                    Some(Update::ChunkBatch(a)) => match read_jsonl(Path::new(&a.path)) {
                        Ok(d) => docs.push(d),
                        Err(e) => errors.push(e),
                    },
                    Some(Update::Error(e)) => errors.push(e),
                    Some(Update::Done(_)) | None => {}
                },
                Ok(None) => break,
                Err(e) => {
                    errors.push(format!("worker: {}", e.message()));
                    break;
                }
            }
        }
        Some((docs, errors))
    }

    /// Ingest als run (zichtbaar in Lopend werk; P3 = op de achtergrond).
    pub async fn ingest(
        &self,
        paths: Vec<String>,
        workspace: Option<String>,
        priority: Priority,
    ) -> Result<(RunId, IngestReport), String> {
        let mem = self.memory.clone().ok_or("geen geheugen")?;
        let mut spec = RunSpec::new(RunKind::Ingest, priority);
        spec.input = json!({"paths": paths, "workspace": workspace});
        let ctx = self.kernel.start_run(spec).map_err(|e| e.to_string())?;
        ctx.start().map_err(|e| e.to_string())?;
        let (run, trace) = (ctx.id(), ctx.trace().trace_id);
        let log = self.kernel.events().clone();
        let emit = |kind: EventKind, p: serde_json::Value| {
            let _ = log.append(Event::new(kind, trace, p).for_run(run));
        };
        emit(EventKind::IngestStarted, json!({"paths": paths, "workspace": workspace}));
        let progress = |f: f64, msg: &str| emit(EventKind::IngestProgress, json!({"fraction": f, "message": msg}));
        let mut rep = IngestReport::default();
        // 1. Parsen: worker (alle formaten) of Rust-fallback.
        let docs = match self.worker_docs(&paths, run, trace, &progress).await {
            Some((d, e)) => {
                rep.via_worker = true;
                rep.errors.extend(e);
                d
            }
            None => {
                let mut d = Vec::new();
                for p in collect_native(&paths) {
                    match parse_native(&p) {
                        Ok(doc) => d.push(doc),
                        Err(e) => rep.errors.push(e),
                    }
                }
                d
            }
        };
        let total = docs.len().max(1);
        // 2. Naar het geheugen (lagere prioriteit dan chat; de Dirigent regelt de GPU-tijd).
        for (i, doc) in docs.into_iter().enumerate() {
            if ctx.is_cancelled() {
                break;
            }
            let title = doc.metadata.get("title").and_then(|t| t.as_str()).unwrap_or("document").to_string();
            progress(0.5 + 0.5 * i as f64 / total as f64, &format!("geheugen: {title}"));
            if mem.store.source_hash(&doc.path).ok().flatten().as_deref() == Some(doc.content_hash.as_str()) {
                rep.skipped += 1;
                continue;
            }
            let mut chapter: Vec<(String, String, Vec<MemoryId>, Vec<String>)> = Vec::new();
            for s in &doc.sections {
                if s.text.trim().len() < 3 {
                    continue;
                }
                let full_title = if s.title.is_empty() { title.clone() } else { format!("{title} › {}", s.title) };
                let mut nm = NewMemory::new(
                    MemoryKind::Semantic,
                    s.text.clone(),
                    Provenance::with_origin(SourceKind::File, doc.path.clone()),
                );
                nm.title = full_title;
                nm.source_uri = Some(doc.path.clone());
                nm.source_hash = Some(doc.content_hash.clone());
                nm.locator = Some(s.locator.clone());
                nm.workspace = workspace.clone();
                nm.tags = vec!["document".into()];
                nm.force = true;
                let ids = match mem.store.write_or_defer(nm).await {
                    Ok(Some(WriteOutcome::Stored { ids, .. })) => ids,
                    Ok(Some(WriteOutcome::Merged { into })) => vec![into],
                    Ok(_) => vec![],
                    Err(e) => {
                        rep.errors.push(format!("{title} {}: {e}", s.locator));
                        vec![]
                    }
                };
                rep.sections += 1;
                // Hoofdstukgrens: EPUB-hoofdstuk uit de locator, anders een kop van niveau 1.
                let ch_key = if s.locator.starts_with("hoofdstuk") {
                    s.locator.split(" > ").next().unwrap_or("").to_string()
                } else if s.level <= 1 && !s.title.is_empty() {
                    s.title.clone()
                } else {
                    chapter.last().map(|c| c.1.clone()).unwrap_or_else(|| "begin".into())
                };
                if chapter.last().is_none_or(|c| c.1 != ch_key) {
                    chapter.push((s.title.clone(), ch_key, vec![], vec![]));
                }
                if let Some(c) = chapter.last_mut() {
                    c.2.extend(ids);
                    c.3.push(s.text.chars().take(3000).collect());
                }
            }
            rep.docs += 1;
            // 3. Hiërarchische samenvattingen: hoofdstuk → boek (W10.05).
            if doc.sections.len() >= 3 {
                let llm = mem.store.llm();
                let mut ch_ids = Vec::new();
                let mut ch_sums = Vec::new();
                for (ct, loc, ids, texts) in chapter.iter().filter(|c| !c.3.is_empty()) {
                    let Ok(sum) = llm.summarize(texts, 120).await else { continue };
                    if sum.trim().is_empty() {
                        continue;
                    }
                    let mut nm = NewMemory::new(
                        MemoryKind::Summary,
                        sum.clone(),
                        Provenance::with_origin(SourceKind::File, doc.path.clone()),
                    );
                    nm.title = format!(
                        "Samenvatting {title}{}",
                        if ct.is_empty() { String::new() } else { format!(" › {ct}") }
                    );
                    nm.locator = Some(loc.clone());
                    nm.derived_from = ids.clone();
                    nm.workspace = workspace.clone();
                    nm.tags = vec!["samenvatting".into(), "hoofdstuk".into()];
                    nm.force = true;
                    if let Ok(WriteOutcome::Stored { ids, .. }) = mem.store.write(nm).await {
                        ch_ids.extend(ids);
                        ch_sums.push(sum);
                        rep.summaries += 1;
                    }
                }
                if ch_sums.len() >= 2
                    && let Ok(book) = llm.summarize(&ch_sums, 200).await
                    && !book.trim().is_empty()
                {
                    let mut nm = NewMemory::new(
                        MemoryKind::Summary,
                        book,
                        Provenance::with_origin(SourceKind::File, doc.path.clone()),
                    );
                    nm.title = format!("Samenvatting {title} (geheel)");
                    nm.locator = Some("geheel".into());
                    nm.derived_from = ch_ids;
                    nm.workspace = workspace.clone();
                    nm.tags = vec!["samenvatting".into(), "boek".into()];
                    nm.force = true;
                    if mem.store.write(nm).await.is_ok() {
                        rep.summaries += 1;
                    }
                }
            }
        }
        emit(EventKind::IngestCompleted, serde_json::to_value(&rep).unwrap_or_default());
        let _ = ctx.complete(serde_json::to_value(&rep).unwrap_or_default());
        Ok((run, rep))
    }
}
