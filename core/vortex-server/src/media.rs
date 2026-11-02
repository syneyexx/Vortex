//! Multimodaal (W16): uploads, bijlagen in de chat (beeld, scan, audio, video, documenten),
//! visie met ROI (overzicht → uitsneden), OCR met onzekerheid, spraak in/uit, beeldgeneratie na
//! het ontladen van het LLM, video-tijdlijn, regio's met coördinaten (visuele retrieval).
//! Alles gaat het geheugen in met een exacte locator (pagina + coördinaten, tijdstempel, cel).
// VX:W16.01

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vortex_cognition::{CogLlm, GenOpts};
use vortex_memory::{MemoryKind, NewMemory};
use vortex_models::backend::Message;
use vortex_types::{Provenance, RunId, SourceKind};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

/// Instellingen (`config/media.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct MediaCfg {
    #[serde(default = "auto")]
    pub ocr_engine: String,
    #[serde(default = "auto")]
    pub stt_engine: String,
    #[serde(default = "small")]
    pub stt_model: String,
    #[serde(default = "auto")]
    pub tts_engine: String,
    #[serde(default)]
    pub tts_voice: Option<String>,
    #[serde(default = "auto")]
    pub imagegen_engine: String,
    #[serde(default)]
    pub imagegen_model: Option<String>,
    /// Overzichtsbeeld (lange zijde, px) en maximaal aantal uitsneden.
    #[serde(default = "overview_px")]
    pub overview_px: u32,
    #[serde(default = "three")]
    pub max_rois: usize,
    /// OCR-regels onder deze zekerheid heten "onzeker".
    #[serde(default = "low")]
    pub ocr_low: f64,
    #[serde(default = "forty")]
    pub video_max_frames: usize,
}
fn auto() -> String {
    "auto".into()
}
fn small() -> String {
    "small".into()
}
fn overview_px() -> u32 {
    1024
}
fn three() -> usize {
    3
}
fn low() -> f64 {
    0.6
}
fn forty() -> usize {
    40
}
impl Default for MediaCfg {
    fn default() -> Self {
        toml::from_str("").unwrap_or_else(|_| Self {
            ocr_engine: auto(),
            stt_engine: auto(),
            stt_model: small(),
            tts_engine: auto(),
            tts_voice: None,
            imagegen_engine: auto(),
            imagegen_model: None,
            overview_px: 1024,
            max_rois: 3,
            ocr_low: 0.6,
            video_max_frames: 40,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Document,
}

pub fn kind_of(path: &Path) -> MediaKind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => MediaKind::Image,
        "wav" | "mp3" | "m4a" | "ogg" | "oga" | "flac" | "opus" => MediaKind::Audio,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" => MediaKind::Video,
        _ => MediaKind::Document,
    }
}

/// Voorbereide context van bijlagen voor één run.
#[derive(Debug, Clone, Default)]
pub struct AttachCtx {
    pub context: String,
    pub images: Vec<String>,
    pub results: Vec<Value>,
}

/// Wachtende bijlage-context per run (gelezen door de cognitieve cyclus).
pub type AttachMap = Arc<parking_lot::Mutex<HashMap<RunId, AttachCtx>>>;

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn encode_png(img: &image::DynamicImage) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let _ = img.write_to(&mut buf, image::ImageFormat::Png);
    buf.into_inner()
}

/// (overzicht als data-URL, uitsneden met pixelvak, beeldgrootte)
pub type Shot = (String, Vec<(String, [u32; 4])>, (u32, u32));

/// Overzicht (verkleind) + uitsneden op volle resolutie (fracties 0..1 van het beeld).
pub fn overview_and_crops(path: &Path, max_px: u32, rois: &[[f64; 4]]) -> Result<Shot, String> {
    let img = image::open(path).map_err(|e| format!("beeld openen: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let ov = if w.max(h) > max_px { img.thumbnail(max_px, max_px) } else { img.clone() };
    let mut crops = Vec::new();
    for r in rois {
        let x = (r[0].clamp(0.0, 1.0) * f64::from(w)) as u32;
        let y = (r[1].clamp(0.0, 1.0) * f64::from(h)) as u32;
        let cw = ((r[2].clamp(0.0, 1.0) * f64::from(w)) as u32).clamp(8, w.saturating_sub(x).max(8));
        let ch = ((r[3].clamp(0.0, 1.0) * f64::from(h)) as u32).clamp(8, h.saturating_sub(y).max(8));
        let c = img.crop_imm(x, y, cw.min(w - x.min(w - 1)), ch.min(h - y.min(h - 1)));
        let c = if c.width().max(c.height()) > max_px { c.thumbnail(max_px, max_px) } else { c };
        crops.push((data_url(&encode_png(&c)), [x, y, cw, ch]));
    }
    Ok((data_url(&encode_png(&ov)), crops, (w, h)))
}

impl ChatEngine {
    pub fn media_cfg(&self) -> MediaCfg {
        self.kernel.config().current().values.get("media").and_then(|v| v.clone().try_into().ok()).unwrap_or_default()
    }

    pub(crate) fn media_dir(&self) -> PathBuf {
        self.data_dir().join("media")
    }

    /// Visiemodel uit de registry (rol `vision`), als er een is.
    pub(crate) fn vision_llm(&self) -> Option<Arc<dyn CogLlm>> {
        let cfg = self.kernel.config().current();
        let reg = vortex_models::Registry::from_config(&cfg).ok()?;
        reg.for_role("vision")?;
        self.role_llm(&cfg, "vision", self.kernel.shutdown_token().child_token())
    }

    async fn job(&self, op: &str, mut cfg: Value) -> Result<Value, String> {
        let out = self
            .media_dir()
            .join("jobs")
            .join(format!("{op}-{}.json", vortex_types::EventId::new().to_string().trim_start_matches("evt_")));
        cfg["result"] = json!(out.display().to_string());
        let (p, _) = self.worker_job(&format!("media:{op}"), crate::world_api::toml_kv(&cfg)).await?;
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&p);
        Ok(v)
    }

    #[allow(clippy::too_many_arguments)] // één plek die alle herinneringsvelden voor media zet
    async fn remember(
        &self,
        kind: MemoryKind,
        title: &str,
        content: &str,
        uri: &Path,
        locator: Option<String>,
        confidence: Option<f64>,
        tags: Vec<String>,
    ) {
        let Some(mem) = &self.memory else { return };
        let mut m = NewMemory::new(kind, content, Provenance::with_origin(SourceKind::File, uri.display().to_string()));
        m.title = title.chars().take(120).collect();
        m.source_uri = Some(uri.display().to_string());
        m.source_hash = Some(format!("media:{}:{}", uri.display(), locator.clone().unwrap_or_default()));
        m.locator = locator;
        m.confidence = confidence;
        m.tags = tags;
        m.force = true;
        let _ = mem.store.write_or_defer(m).await;
    }

    /// W16.01: kijken met ROI — eerst het overzicht (waar zit het relevante?), dan uitsneden
    /// op volle resolutie. Geeft de beschrijving + gebruikte uitsneden.
    pub async fn look(&self, path: &Path, question: &str) -> Result<Value, String> {
        let llm = self.vision_llm().ok_or("geen visiemodel (rol \"vision\" in models.toml)")?;
        let cfg = self.media_cfg();
        let (ov, _, (w, h)) = overview_and_crops(path, cfg.overview_px, &[])?;
        let opts =
            |max| GenOpts { max_tokens: Some(max), temperature: Some(0.1), json_schema: None, reasoning_effort: None };
        // Pass 1: overzicht + relevante gebieden.
        let p1 = llm
            .complete(
                vec![
                    Message::system("Je ziet een verkleind overzicht van een afbeelding. Beschrijf kort wat je ziet en geef maximaal 3 gebieden die je in detail moet bekijken om de vraag te beantwoorden (kleine tekst, grafiekassen, details), als fracties van breedte/hoogte. JSON: {\"overview\": \"…\", \"regions\": [{\"x\": 0..1, \"y\": 0..1, \"w\": 0..1, \"h\": 0..1, \"why\": \"…\"}]}"),
                    Message::user_with_images(format!("Vraag: {question}"), vec![ov.clone()]),
                ],
                opts(400),
            )
            .await
            .map_err(|e| format!("visie: {e}"))?;
        let v = vortex_agents::json::extract(&p1).unwrap_or_else(|| json!({"overview": p1, "regions": []}));
        let rois: Vec<[f64; 4]> = v["regions"]
            .as_array()
            .into_iter()
            .flatten()
            .take(cfg.max_rois)
            .filter_map(|r| Some([r["x"].as_f64()?, r["y"].as_f64()?, r["w"].as_f64()?, r["h"].as_f64()?]))
            .filter(|r| r[2] > 0.01 && r[3] > 0.01)
            .collect();
        let overview_text = v["overview"].as_str().unwrap_or(&p1).to_string();
        if rois.is_empty() {
            return Ok(json!({"description": overview_text, "regions": [], "size": [w, h], "image": ov}));
        }
        let (_, crops, _) = overview_and_crops(path, cfg.overview_px, &rois)?;
        // Pass 2: details uit de uitsneden.
        let p2 = llm
            .complete(
                vec![
                    Message::system("Eerst het overzicht, daarna uitsneden op volle resolutie. Beschrijf de afbeelding nauwkeurig in het Nederlands voor de vraag: alle relevante tekst letterlijk, getallen, assen en legenda's van grafieken. Zeg wat onleesbaar of onzeker is."),
                    Message::user_with_images(format!("Vraag: {question}\nOverzicht: {overview_text}"), std::iter::once(ov.clone()).chain(crops.iter().map(|c| c.0.clone())).collect()),
                ],
                opts(900),
            )
            .await
            .map_err(|e| format!("visie: {e}"))?;
        Ok(
            json!({"description": p2, "overview": overview_text, "regions": crops.iter().map(|c| json!({"x": c.1[0], "y": c.1[1], "w": c.1[2], "h": c.1[3]})).collect::<Vec<_>>(), "size": [w, h], "image": ov}),
        )
    }

    /// Eén bijlage verwerken → tekst voor de prompt + geheugen. `question` stuurt visie/ROI.
    pub async fn process_attachment(
        &self,
        path: &Path,
        question: &str,
        workspace: Option<String>,
    ) -> Result<(String, Vec<String>, Value), String> {
        if !path.exists() {
            return Err(format!("bijlage niet gevonden: {}", path.display()));
        }
        let cfg = self.media_cfg();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("bijlage").to_string();
        match kind_of(path) {
            MediaKind::Image => {
                // Visie als het kan, OCR erbij voor tekst met zekerheid (scans, screenshots).
                let seen = self.look(path, question).await;
                let ocr = self
                    .job(
                        "ocr",
                        json!({"path": path.display().to_string(), "engine": cfg.ocr_engine, "low": cfg.ocr_low}),
                    )
                    .await;
                let mut parts = Vec::new();
                let mut images = Vec::new();
                let mut conf = None;
                if let Ok(s) = &seen {
                    parts.push(format!("Wat er te zien is: {}", s["description"].as_str().unwrap_or("")));
                    if let Some(i) = s["image"].as_str() {
                        images.push(i.to_string());
                    }
                }
                if let Ok(o) = &ocr
                    && !o["text"].as_str().unwrap_or("").is_empty()
                {
                    conf = o["mean_conf"].as_f64();
                    parts.push(format!(
                        "Gelezen tekst (OCR, zekerheid {:.0}%):\n{}",
                        conf.unwrap_or(0.0) * 100.0,
                        o["text"].as_str().unwrap_or("")
                    ));
                    let unsure: Vec<String> = o["uncertain"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|l| {
                            Some(format!("\"{}\" ({:.0}%)", l["text"].as_str()?, l["conf"].as_f64()? * 100.0))
                        })
                        .collect();
                    if !unsure.is_empty() {
                        parts.push(format!(
                            "ONZEKER gelezen (benoem dit in je antwoord, gok niet): {}",
                            unsure.join(", ")
                        ));
                    }
                }
                if parts.is_empty() {
                    return Err(format!(
                        "afbeelding niet te verwerken: {} / {}",
                        seen.err().unwrap_or_default(),
                        ocr.err().unwrap_or_default()
                    ));
                }
                let text = parts.join("\n");
                self.remember(
                    MemoryKind::Semantic,
                    &format!("Afbeelding {name}"),
                    &text,
                    path,
                    Some("afbeelding".into()),
                    conf,
                    vec!["media:image".into()],
                )
                .await;
                let _ = workspace;
                Ok((
                    format!("Bijlage {name} (afbeelding):\n{text}"),
                    images,
                    json!({"kind": "image", "vision": seen.is_ok(), "ocr": ocr.ok().map(|o| json!({"mean_conf": o["mean_conf"], "uncertain": o["uncertain"]}))}),
                ))
            }
            MediaKind::Audio => {
                let t = self
                    .job(
                        "stt",
                        json!({"path": path.display().to_string(), "engine": cfg.stt_engine, "model": cfg.stt_model}),
                    )
                    .await?;
                let segs = t["segments"].as_array().cloned().unwrap_or_default();
                // Geheugen per blok van ~30 s met tijdstempel-locator.
                for chunk in group_segments(&segs, 30.0) {
                    self.remember(
                        MemoryKind::Episodic,
                        &format!("Opname {name} {}", chunk.0),
                        &chunk.1,
                        path,
                        Some(chunk.0.clone()),
                        chunk.2,
                        vec!["media:audio".into()],
                    )
                    .await;
                }
                let text = segs
                    .iter()
                    .map(|s| {
                        format!("[{}] {}", ts(s["start"].as_f64().unwrap_or(0.0)), s["text"].as_str().unwrap_or(""))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok((
                    format!("Bijlage {name} (audio, transcript):\n{text}"),
                    vec![],
                    json!({"kind": "audio", "segments": segs.len(), "engine": t["engine"]}),
                ))
            }
            MediaKind::Video => {
                let dir = self
                    .media_dir()
                    .join("video")
                    .join(vortex_tools::stable_hash(path.display().to_string().as_bytes()).to_string());
                let v = self.job("video", json!({"path": path.display().to_string(), "out_dir": dir.display().to_string(), "stt_engine": cfg.stt_engine, "max_frames": cfg.video_max_frames})).await?;
                let vision = self.vision_llm();
                let mut lines = Vec::new();
                for f in v["frames"].as_array().into_iter().flatten() {
                    let t0 = f["t"].as_f64().unwrap_or(0.0);
                    let t1 = f["end"].as_f64().unwrap_or(t0);
                    let mut seen = String::new();
                    if vision.is_some()
                        && let Some(p) = f["path"].as_str()
                        && let Ok(d) = self.look(Path::new(p), question).await
                    {
                        seen = d["description"].as_str().unwrap_or("").to_string();
                    }
                    let speech = f["speech"].as_str().unwrap_or("");
                    let line = format!(
                        "[{}–{}] beeld: {} · spraak: {}",
                        ts(t0),
                        ts(t1),
                        if seen.is_empty() { "(geen visiemodel)" } else { &seen },
                        if speech.is_empty() { "–" } else { speech }
                    );
                    self.remember(
                        MemoryKind::Episodic,
                        &format!("Video {name} {}", ts(t0)),
                        &line,
                        path,
                        Some(format!("{}–{}", ts(t0), ts(t1))),
                        None,
                        vec!["media:video".into()],
                    )
                    .await;
                    lines.push(line);
                }
                Ok((
                    format!(
                        "Bijlage {name} (video, {:.0} s, tijdlijn):\n{}",
                        v["duration"].as_f64().unwrap_or(0.0),
                        lines.join("\n")
                    ),
                    vec![],
                    json!({"kind": "video", "frames": lines.len(), "scene_cuts": v["scene_cuts"]}),
                ))
            }
            MediaKind::Document => {
                let (_, rep) = self
                    .ingest(vec![path.display().to_string()], workspace.clone(), vortex_types::Priority::P1)
                    .await?;
                let regions = self.index_regions(path).await.unwrap_or(0);
                Ok((
                    format!(
                        "Bijlage {name} (document) is ingelezen in het geheugen; relevante passages (met pagina/coördinaten of cel) komen via het geheugen mee. Noem de locator bij citaten."
                    ),
                    vec![],
                    json!({"kind": "document", "ingest": rep, "regions": regions}),
                ))
            }
        }
    }

    /// W16.04: regio's (blokken, tabellen) met coördinaten als eigen herinneringen; gescande
    /// pagina's → renderen → OCR/visie. Antwoorden citeren zo "p. 3 [x0, y0, x1, y1]".
    // VX:W16.04
    pub async fn index_regions(&self, path: &Path) -> Result<usize, String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !["pdf", "docx", "xlsx", "xlsm", "csv", "tsv"].contains(&ext.as_str()) {
            return Ok(0);
        }
        let r = self.job("regions", json!({"path": path.display().to_string()})).await?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("document").to_string();
        let cfg = self.media_cfg();
        let mut n = 0;
        for reg in r["regions"].as_array().into_iter().flatten() {
            let loc = reg["locator"].as_str().unwrap_or("").to_string();
            match reg["kind"].as_str() {
                Some("scan") if ext == "pdf" => {
                    let page = reg["page"].as_u64().unwrap_or(1);
                    let png = self.media_dir().join("pages").join(format!(
                        "{}_p{page}.png",
                        vortex_tools::stable_hash(path.display().to_string().as_bytes())
                    ));
                    if self
                        .job(
                            "render_page",
                            json!({"path": path.display().to_string(), "page": page, "out": png.display().to_string()}),
                        )
                        .await
                        .is_err()
                    {
                        continue;
                    }
                    let mut text = String::new();
                    let mut conf = None;
                    if let Ok(o) = self
                        .job(
                            "ocr",
                            json!({"path": png.display().to_string(), "engine": cfg.ocr_engine, "low": cfg.ocr_low}),
                        )
                        .await
                    {
                        text = o["text"].as_str().unwrap_or("").to_string();
                        conf = o["mean_conf"].as_f64();
                    }
                    if text.trim().is_empty()
                        && let Ok(d) = self.look(&png, "Lees alle tekst op deze pagina letterlijk.").await
                    {
                        text = d["description"].as_str().unwrap_or("").to_string();
                        conf = Some(0.5);
                    }
                    if !text.trim().is_empty() {
                        self.remember(
                            MemoryKind::Semantic,
                            &format!("{name} p. {page} (scan)"),
                            &text,
                            path,
                            Some(format!("p. {page} (scan)")),
                            conf,
                            vec!["media:region".into()],
                        )
                        .await;
                        n += 1;
                    }
                }
                Some("text" | "table" | "heading") => {
                    let text = reg["text"].as_str().unwrap_or("");
                    if text.chars().count() < 25 {
                        continue;
                    }
                    self.remember(
                        MemoryKind::Semantic,
                        &format!("{name} {loc}"),
                        text,
                        path,
                        Some(loc),
                        None,
                        vec!["media:region".into(), format!("region:{}", reg["kind"].as_str().unwrap_or(""))],
                    )
                    .await;
                    n += 1;
                }
                _ => {}
            }
        }
        Ok(n)
    }

    /// Alle bijlagen van een bericht verwerken (vóór de cognitieve cyclus).
    // VX:W16.08
    pub async fn prepare_attachments(&self, run: RunId, paths: &[String], question: &str, workspace: Option<String>) {
        let mut ctx = AttachCtx::default();
        for p in paths {
            match self.process_attachment(Path::new(p), question, workspace.clone()).await {
                Ok((text, images, res)) => {
                    ctx.context.push_str(&text);
                    ctx.context.push_str("\n\n");
                    ctx.images.extend(images);
                    ctx.results.push(json!({"path": p, "ok": true, "result": res}));
                }
                Err(e) => {
                    ctx.context.push_str(&format!("Bijlage {p} kon niet verwerkt worden: {e}. Zeg dat eerlijk.\n\n"));
                    ctx.results.push(json!({"path": p, "ok": false, "error": e}));
                }
            }
        }
        let _ = self.kernel.events().append(
            vortex_types::Event::new(
                vortex_types::EventKind::AttachmentsProcessed,
                vortex_types::TraceId::new(),
                json!({"items": ctx.results}),
            )
            .for_run(run),
        );
        self.attachments.lock().insert(run, ctx);
    }

    /// Chatantwoord = gegenereerd (of bewerkt) beeld.
    pub(crate) async fn answer_with_image(
        &self,
        ctx: vortex_core::RunContext,
        session: vortex_types::SessionId,
        msg: vortex_types::MessageId,
        prompt: &str,
        init: Option<String>,
    ) {
        let _ = ctx.start();
        let (run, trace) = (ctx.id(), ctx.trace().trace_id);
        let r = self.imagine(prompt, init).await;
        let (text, status, err) = match &r {
            Ok(v) => (
                format!(
                    "![{}]({})\n\n*Beeld gemaakt ({}).*",
                    prompt.replace(['[', ']'], ""),
                    v["url"].as_str().unwrap_or(""),
                    v["engine"].as_str().unwrap_or("?")
                ),
                "complete",
                None,
            ),
            Err(e) => (String::new(), "failed", Some(e.clone())),
        };
        let log = self.kernel.events().clone();
        let _ = log.transact(|tx, out| {
            crate::sessions::SessionStore::finish_tx(tx, msg, &text, "", status, None)?;
            if !text.is_empty() {
                out.emit(
                    tx,
                    vortex_types::Event::new(
                        vortex_types::EventKind::MessageDelta,
                        trace,
                        json!({"session_id": session, "message_id": msg, "text": text, "reasoning": ""}),
                    )
                    .for_run(run),
                )?;
            }
            out.emit(
                tx,
                vortex_types::Event::new(
                    vortex_types::EventKind::MessageCompleted,
                    trace,
                    json!({"session_id": session, "message_id": msg, "status": status, "error": err}),
                )
                .for_run(run),
            )?;
            Ok(())
        });
        let _ = match err {
            None => ctx.complete(json!({"message_id": msg, "image": r.ok()})).map(|_| ()),
            Some(e) => ctx.fail(&e).map(|_| ()),
        };
    }

    /// W16.05: tekst → spraak (per zin, onderbreekbaar in de GUI).
    pub async fn speak(&self, text: &str) -> Result<Value, String> {
        let cfg = self.media_cfg();
        let out =
            self.media_dir().join("files").join(format!("tts_{}.wav", vortex_tools::stable_hash(text.as_bytes())));
        let r = self.job("tts", json!({"text": text, "out": out.display().to_string(), "engine": cfg.tts_engine, "voice": cfg.tts_voice})).await?;
        let parts: Vec<Value> = r["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let f = Path::new(p["path"].as_str().unwrap_or(""))
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                json!({"text": p["text"], "url": format!("/api/media/file/{f}"), "seconds": p["seconds"]})
            })
            .collect();
        Ok(json!({"parts": parts}))
    }

    /// W16.06: beeld genereren of bewerken. Eerst het LLM van de GPU (beheerde servers
    /// ontladen), daarna terugzetten; GPU-lease P1 via de Dirigent.
    // VX:W16.06
    pub async fn imagine(&self, prompt: &str, init_image: Option<String>) -> Result<Value, String> {
        let cfg = self.media_cfg();
        let mut spec =
            vortex_core::RunSpec::new(vortex_types::RunKind::Other("imagegen".into()), vortex_types::Priority::P1);
        spec.input = json!({"prompt": prompt, "edit": init_image.is_some()});
        let ctx = self.kernel.start_run(spec).map_err(|e| e.to_string())?;
        ctx.start().map_err(|e| e.to_string())?;
        let r = async {
            let _gpu = ctx.gpu(vortex_types::Priority::P1).await.map_err(|e| e.to_string())?;
            let released = self.gpu_guard.unload_all().await;
            let out = self.media_dir().join("files").join(format!("img_{}.png", vortex_types::EventId::new().to_string().trim_start_matches("evt_")));
            let r = self.job("imagegen", json!({"prompt": prompt, "out": out.display().to_string(), "engine": cfg.imagegen_engine, "model": cfg.imagegen_model, "init_image": init_image})).await;
            if released > 0 {
                let _ = self.gpu_guard.load_all().await;
            }
            let r = r?;
            let f = out.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            self.remember(MemoryKind::Episodic, &format!("Gegenereerd beeld: {}", prompt.chars().take(60).collect::<String>()), &format!("Beeld gemaakt met prompt: {prompt}"), &out, Some("afbeelding".into()), None, vec!["media:generated".into()]).await;
            Ok::<_, String>(json!({"url": format!("/api/media/file/{f}"), "path": out.display().to_string(), "engine": r["engine"], "llm_unloaded": released}))
        }
        .await;
        match &r {
            Ok(v) => {
                let _ = ctx.complete(v.clone());
            }
            Err(e) => {
                let _ = ctx.fail(e);
            }
        }
        r
    }
}

fn ts(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Segmenten bundelen tot blokken van ~`secs` (locator, tekst, gemiddelde zekerheid).
fn group_segments(segs: &[Value], secs: f64) -> Vec<(String, String, Option<f64>)> {
    let mut out = Vec::new();
    let (mut start, mut end, mut text, mut confs) = (None::<f64>, 0.0, String::new(), Vec::new());
    for s in segs {
        let (a, b) = (s["start"].as_f64().unwrap_or(0.0), s["end"].as_f64().unwrap_or(0.0));
        if start.is_some_and(|st| b - st > secs) {
            out.push((
                format!("{}–{}", ts(start.unwrap_or(0.0)), ts(end)),
                text.trim().to_string(),
                (!confs.is_empty()).then(|| confs.iter().sum::<f64>() / confs.len() as f64),
            ));
            (start, text, confs) = (None, String::new(), Vec::new());
        }
        start.get_or_insert(a);
        end = b;
        text.push_str(s["text"].as_str().unwrap_or(""));
        text.push(' ');
        if let Some(c) = s["conf"].as_f64() {
            confs.push(c);
        }
    }
    if let Some(st) = start {
        out.push((
            format!("{}–{}", ts(st), ts(end)),
            text.trim().to_string(),
            (!confs.is_empty()).then(|| confs.iter().sum::<f64>() / confs.len() as f64),
        ));
    }
    out
}

// ── REST ─────────────────────────────────────────────────────────────────────

fn e400(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.to_string())
}

#[derive(Debug, Deserialize)]
struct Upload {
    name: String,
    /// Base64 (zonder of met data-URL-prefix).
    data: String,
}

fn safe_name(n: &str) -> String {
    let base = Path::new(n).file_name().and_then(|x| x.to_str()).unwrap_or("bestand");
    base.chars().map(|c| if c.is_alphanumeric() || ".-_".contains(c) { c } else { '_' }).collect()
}

async fn upload(State(s): State<AppState>, Json(b): Json<Upload>) -> Result<Json<Value>, ApiError> {
    let raw = b.data.split_once("base64,").map_or(b.data.as_str(), |x| x.1);
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw.trim()).map_err(e400)?;
    let dir = s.chat.data_dir().join("uploads");
    std::fs::create_dir_all(&dir).map_err(e400)?;
    let p = dir.join(format!("{:012x}_{}", vortex_tools::stable_hash(&bytes) & 0xffff_ffff_ffff, safe_name(&b.name)));
    std::fs::write(&p, &bytes).map_err(e400)?;
    Ok(Json(json!({"path": p.display().to_string(), "kind": kind_of(&p), "bytes": bytes.len()})))
}

#[derive(Debug, Deserialize)]
struct Analyze {
    path: String,
    #[serde(default)]
    question: String,
}

async fn analyze(State(s): State<AppState>, Json(b): Json<Analyze>) -> Result<Json<Value>, ApiError> {
    let (text, _, res) = s.chat.process_attachment(Path::new(&b.path), &b.question, None).await.map_err(e400)?;
    Ok(Json(json!({"text": text, "result": res})))
}

async fn transcribe(State(s): State<AppState>, Json(b): Json<Upload>) -> Result<Json<Value>, ApiError> {
    let Json(up) = upload(State(s.clone()), Json(b)).await?;
    let cfg = s.chat.media_cfg();
    let t = s
        .chat
        .job("stt", json!({"path": up["path"], "engine": cfg.stt_engine, "model": cfg.stt_model}))
        .await
        .map_err(e400)?;
    Ok(Json(json!({"text": t["text"], "segments": t["segments"], "engine": t["engine"]})))
}

#[derive(Debug, Deserialize)]
struct Speak {
    text: String,
}

async fn speak(State(s): State<AppState>, Json(b): Json<Speak>) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.chat.speak(&b.text).await.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct Imagine {
    prompt: String,
    #[serde(default)]
    init_image: Option<String>,
}

async fn imagine(State(s): State<AppState>, Json(b): Json<Imagine>) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.chat.imagine(&b.prompt, b.init_image).await.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct PageQ {
    path: String,
    page: u32,
}

/// Pagina als PNG (voor de bronweergave met gemarkeerde coördinaten).
async fn page(State(s): State<AppState>, Json(b): Json<PageQ>) -> Result<Json<Value>, ApiError> {
    let out = s.chat.media_dir().join("files").join(format!(
        "page_{}_{}.png",
        vortex_tools::stable_hash(b.path.as_bytes()),
        b.page
    ));
    let r = s
        .chat
        .job("render_page", json!({"path": b.path, "page": b.page, "out": out.display().to_string(), "zoom": 1.5}))
        .await
        .map_err(e400)?;
    let f = out.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    Ok(Json(
        json!({"url": format!("/api/media/file/{f}"), "width": r["width"], "height": r["height"], "zoom": r["zoom"], "page_size": r["page_size"]}),
    ))
}

async fn file(State(s): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    let p = s.chat.media_dir().join("files").join(safe_name(&name));
    match std::fs::read(&p) {
        Ok(b) => {
            let ct = match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "wav" => "audio/wav",
                _ => "application/octet-stream",
            };
            ([(header::CONTENT_TYPE, ct)], b).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "niet gevonden").into_response(),
    }
}

async fn status(State(s): State<AppState>) -> Json<Value> {
    let c = s.chat.media_cfg();
    Json(
        json!({"vision": s.chat.vision_llm().is_some(), "ocr_engine": c.ocr_engine, "stt_engine": c.stt_engine, "tts_engine": c.tts_engine, "imagegen_engine": c.imagegen_engine}),
    )
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/media", get(status))
        .route("/api/uploads", post(upload))
        .route("/api/media/analyze", post(analyze))
        .route("/api/media/transcribe", post(transcribe))
        .route("/api/media/speak", post(speak))
        .route("/api/media/imagine", post(imagine))
        .route("/api/media/page", post(page))
        .route("/api/media/file/:name", get(file))
}
