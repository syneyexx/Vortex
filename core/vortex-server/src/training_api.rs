//! Training en vliegwiel (W13): datasetbouwers (SFT, DPO, Q&A), adapter-registry, QLoRA via de
//! worker (alleen in idle; pauzeren bij gebruik, hervatten vanaf checkpoint), hot-swap van
//! adapters in llama-server, poortwachter (eval zonder regressie + schaduwrun), kalibratie van
//! de Judge-zekerheid, en de API voor de Training-pagina.
// VX:W13.01

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_core::RunSpec;
use vortex_neural::calibration::Calibrator;
use vortex_proto::common::v1::{CancelRequest, TraceContext};
use vortex_proto::worker::v1::worker_service_client::WorkerServiceClient;
use vortex_proto::worker::v1::{TrainRequest, train_update::Update};
use vortex_types::{Event, EventId, EventKind, Priority, RunKind, TraceId};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

/// Trainingsinstellingen uit `config/training.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct TrainCfg {
    #[serde(default)]
    pub base_model: String,
    #[serde(default)]
    pub serve_model: Option<String>,
    #[serde(default = "out")]
    pub out_dir: String,
    #[serde(default)]
    pub llama_cpp: Option<String>,
    #[serde(default = "yes")]
    pub idle: bool,
    #[serde(default = "minn")]
    pub min_new_examples: usize,
    #[serde(default)]
    pub lora: toml::Table,
    #[serde(default)]
    pub gate: GateCfg,
}
fn out() -> String {
    "training".into()
}
fn yes() -> bool {
    true
}
fn minn() -> usize {
    200
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateCfg {
    #[serde(default = "g02")]
    pub min_gain: f64,
    #[serde(default = "g02")]
    pub max_regression: f64,
    #[serde(default = "s5")]
    pub shadow_min: usize,
    #[serde(default = "h05")]
    pub shadow_win_rate: f64,
}
impl Default for GateCfg {
    fn default() -> Self {
        Self { min_gain: 0.02, max_regression: 0.02, shadow_min: 5, shadow_win_rate: 0.5 }
    }
}
fn g02() -> f64 {
    0.02
}
fn s5() -> usize {
    5
}
fn h05() -> f64 {
    0.5
}

impl ChatEngine {
    pub fn train_cfg(&self) -> TrainCfg {
        let snap = self.kernel.config().current();
        snap.values.get("training").and_then(|v| v.clone().try_into().ok()).unwrap_or(TrainCfg {
            base_model: String::new(),
            serve_model: None,
            out_dir: out(),
            llama_cpp: None,
            idle: true,
            min_new_examples: minn(),
            lora: toml::Table::new(),
            gate: GateCfg::default(),
        })
    }

    /// Gefitte kalibrator (als er een is).
    pub fn calibrator(&self) -> Option<Calibrator> {
        Calibrator::load(&self.training_dir().join("calibrator.json"))
    }

    fn training_dir(&self) -> PathBuf {
        let c = self.train_cfg();
        let p = PathBuf::from(&c.out_dir);
        if p.is_relative() { self.data_dir().join(p) } else { p }
    }
}

fn db_rows(log: &vortex_core::EventLog, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Vec<(Option<String>, String)> {
    log.db()
        .read(|c| {
            let mut st = c.prepare(sql)?;
            let v = st.query_map(p, |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default()
}

fn feedback_by_run(log: &vortex_core::EventLog) -> HashMap<String, String> {
    db_rows(log, "SELECT run_id, rating FROM feedback WHERE run_id IS NOT NULL", &[])
        .into_iter()
        .filter_map(|(r, x)| r.map(|r| (r, x)))
        .collect()
}

/// Dataset wegschrijven: dedup, decontaminatie t.o.v. de eval-set, versie = inhoudshash.
pub(crate) fn save_dataset(
    chat: &ChatEngine,
    kind: &str,
    rows: Vec<Value>,
    text_of: impl Fn(&Value) -> String,
    stats: Value,
) -> Result<Value, String> {
    let sets = vortex_eval::load_dir(&crate::eval_api::evals_dir().join("sets")).unwrap_or_default();
    let eval = vortex_neural::dataset::eval_index(
        &sets.into_iter().flat_map(|s| s.tasks.into_iter().map(|t| t.prompt)).collect::<Vec<_>>(),
    );
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    let mut dropped = 0;
    for r in rows {
        let t = text_of(&r);
        if vortex_neural::dataset::contaminated(&t, &eval) {
            dropped += 1;
            continue;
        }
        if seen.insert(vortex_tools::stable_hash(t.as_bytes())) {
            kept.push(r);
        }
    }
    let body: String = kept.iter().map(|r| format!("{r}\n")).collect();
    let version = format!("{kind}-{:012x}", vortex_tools::stable_hash(body.as_bytes()) & 0xffff_ffff_ffff);
    let dir = chat.training_dir().join("datasets");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{version}.jsonl"));
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    let now = vortex_types::now().to_rfc3339();
    let log = chat.kernel.events();
    log.transact(|tx, out| {
        tx.execute(
            "INSERT OR REPLACE INTO training_datasets(version, kind, path, n, dropped, stats, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![version, kind, path.display().to_string(), kept.len() as i64, dropped as i64, stats.to_string(), now],
        )?;
        out.emit(tx, Event::new(EventKind::DatasetBuilt, TraceId::new(), json!({"version": version, "kind": kind, "n": kept.len(), "decontaminated": dropped})))?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(
        json!({"version": version, "kind": kind, "n": kept.len(), "decontaminated": dropped, "path": path, "stats": stats}),
    )
}

/// SFT (W13.01): distillatie van door de Judge goedgekeurde team-antwoorden (zonder negatieve
/// feedback), goed beoordeelde antwoorden, en correcties van de gebruiker.
pub fn build_sft(chat: &ChatEngine) -> Result<Value, String> {
    let log = chat.kernel.events();
    let fb = feedback_by_run(log);
    let sys = chat.kernel.config().current().file("prompts/system.md").unwrap_or("").trim().to_string();
    let mut rows = Vec::new();
    let (mut team, mut good, mut corr) = (0, 0, 0);
    for (run, p) in db_rows(log, "SELECT run_id, payload FROM events WHERE kind = 'agent.candidates.recorded'", &[]) {
        let v: Value = serde_json::from_str(&p).unwrap_or_default();
        let ok = v["verdict"]["verdict"] == "accept" && v["verdict"]["confidence"].as_f64().unwrap_or(0.0) >= 0.7;
        let bad = run.as_ref().and_then(|r| fb.get(r)).is_some_and(|r| r != "good");
        if let (true, false, Some(c)) =
            (ok, bad, v["candidates"].as_array().and_then(|a| a.iter().find(|c| c["chosen"] == true)))
        {
            rows.push(json!({"messages": [{"role": "system", "content": sys}, {"role": "user", "content": v["question"]}, {"role": "assistant", "content": c["text"]}], "source": "team", "run": run}));
            team += 1;
        }
    }
    let msgs: Vec<(Option<String>, String)> = db_rows(
        log,
        "SELECT f.run_id, f.rating || char(30) || COALESCE(f.correction, '') || char(30) || m.content FROM feedback f JOIN messages m ON m.id = f.message_id",
        &[],
    );
    for (run, packed) in msgs {
        let mut it = packed.split('\u{1e}');
        let (rating, correction, answer) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
        let Some(q) = run.and_then(|r| r.parse().ok()).and_then(|r| chat.question_of(r)) else { continue };
        match rating {
            "good" if !answer.trim().is_empty() => {
                rows.push(json!({"messages": [{"role": "system", "content": sys}, {"role": "user", "content": q}, {"role": "assistant", "content": answer}], "source": "feedback"}));
                good += 1;
            }
            "correction" if !correction.trim().is_empty() => {
                rows.push(json!({"messages": [{"role": "system", "content": sys}, {"role": "user", "content": q}, {"role": "assistant", "content": correction}], "source": "correctie"}));
                corr += 1;
            }
            _ => {}
        }
    }
    save_dataset(
        chat,
        "sft",
        rows,
        |r| r["messages"][1]["content"].as_str().unwrap_or("").to_string(),
        json!({"team": team, "good": good, "corrections": corr}),
    )
}

/// DPO (W13.01): gekozen vs verworpen. `label` = "correctness" (objectieve check/Judge) of
/// "preference" (gebruikersfeedback) — apart gehouden.
pub fn build_dpo(chat: &ChatEngine) -> Result<Value, String> {
    let log = chat.kernel.events();
    let mut rows = Vec::new();
    let (mut corr, mut pref) = (0, 0);
    for (_, p) in db_rows(log, "SELECT run_id, payload FROM events WHERE kind = 'agent.candidates.recorded'", &[]) {
        let v: Value = serde_json::from_str(&p).unwrap_or_default();
        let Some(cs) = v["candidates"].as_array() else { continue };
        let Some(ch) = cs.iter().find(|c| c["chosen"] == true) else { continue };
        let Some(rej) = cs
            .iter()
            .filter(|c| c["chosen"] != true && !c["text"].as_str().unwrap_or("").trim().is_empty())
            .min_by(|a, b| a["score"].as_f64().unwrap_or(0.0).total_cmp(&b["score"].as_f64().unwrap_or(0.0)))
        else {
            continue;
        };
        let gap = ch["score"].as_f64().unwrap_or(0.0) - rej["score"].as_f64().unwrap_or(0.0);
        if gap < 0.15 {
            continue; // te weinig verschil: geen betrouwbaar paar
        }
        let objective = ch["objective"] == true || rej["objective"] == false;
        rows.push(json!({"prompt": v["question"], "chosen": ch["text"], "rejected": rej["text"], "label": if objective { "correctness" } else { "judge" }, "margin": gap}));
        corr += 1;
    }
    // Correcties: wat de gebruiker zei = gekozen, oorspronkelijk antwoord = verworpen (voorkeur/feit).
    let msgs: Vec<(Option<String>, String)> = db_rows(
        log,
        "SELECT f.run_id, COALESCE(f.correction, '') || char(30) || m.content FROM feedback f JOIN messages m ON m.id = f.message_id WHERE f.rating = 'correction'",
        &[],
    );
    for (run, packed) in msgs {
        let (c, a) = packed.split_once('\u{1e}').unwrap_or(("", ""));
        let Some(q) = run.and_then(|r| r.parse().ok()).and_then(|r| chat.question_of(r)) else { continue };
        if !c.trim().is_empty() && !a.trim().is_empty() {
            rows.push(json!({"prompt": q, "chosen": c, "rejected": a, "label": "preference"}));
            pref += 1;
        }
    }
    save_dataset(
        chat,
        "dpo",
        rows,
        |r| r["prompt"].as_str().unwrap_or("").to_string(),
        json!({"team_pairs": corr, "corrections": pref}),
    )
}

/// Q&A- en redeneervoorbeelden uit ingelezen documenten (geen ruwe tekst): per sectie laat het
/// LM 1–2 vragen + brongebonden antwoorden maken (met locator).
pub async fn build_qa(
    chat: &ChatEngine,
    max_sections: usize,
    llm: std::sync::Arc<dyn vortex_cognition::CogLlm>,
) -> Result<Value, String> {
    let mem = chat.memory.as_ref().ok_or("geen geheugen")?;
    let docs = mem.store.documents(None).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut used = 0;
    'outer: for d in docs {
        let uri = d["uri"].as_str().unwrap_or("");
        for m in mem
            .store
            .by_source(uri)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.kind != vortex_memory::MemoryKind::Summary)
        {
            if used >= max_sections {
                break 'outer;
            }
            used += 1;
            let out = llm
                .complete(
                    vec![
                        vortex_models::backend::Message::system("Maak 1 tot 2 vraag-antwoordparen die met deze tekst te beantwoorden zijn, met een korte redenering. Alleen wat er staat. JSON: {\"pairs\": [{\"q\": \"…\", \"reasoning\": \"…\", \"a\": \"…\"}]}."),
                        vortex_models::backend::Message::user(m.content.chars().take(3000).collect::<String>()),
                    ],
                    vortex_cognition::GenOpts { max_tokens: Some(600), temperature: Some(0.3), json_schema: None, reasoning_effort: None },
                )
                .await
                .unwrap_or_default();
            let v = vortex_agents::json::extract(&out).unwrap_or_default();
            for p in v["pairs"].as_array().into_iter().flatten() {
                let (q, a) = (p["q"].as_str().unwrap_or(""), p["a"].as_str().unwrap_or(""));
                if q.len() > 8 && a.len() > 2 {
                    rows.push(json!({
                        "messages": [{"role": "user", "content": q}, {"role": "assistant", "content": format!("{}\n\n{a}", p["reasoning"].as_str().unwrap_or("")).trim().to_string()}],
                        "source": uri, "locator": m.locator
                    }));
                }
            }
        }
    }
    save_dataset(
        chat,
        "qa",
        rows,
        |r| r["messages"][0]["content"].as_str().unwrap_or("").to_string(),
        json!({"sections": used}),
    )
}

fn adapter_set(log: &vortex_core::EventLog, id: &str, sets: &[(&str, String)]) {
    let now = vortex_types::now().to_rfc3339();
    let _ = log.db().write(|tx| {
        for (k, v) in sets {
            tx.execute(&format!("UPDATE adapters SET {k} = ?2, updated_at = ?3 WHERE id = ?1"), params![id, v, now])?;
        }
        Ok(())
    });
}

fn adapter(log: &vortex_core::EventLog, id: &str) -> Option<Value> {
    log.db()
        .read(|c| {
            Ok(c.query_row(
                "SELECT id, base_model, kind, dataset, settings, adapter_dir, gguf_path, status, scores, regressions, shadow, checkpoint, metrics, created_at, updated_at FROM adapters WHERE id = ?1",
                [id],
                |r| {
                    let j = |i: usize| -> rusqlite::Result<Value> { Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or(Value::Null)) };
                    Ok(json!({"id": r.get::<_, String>(0)?, "base_model": r.get::<_, String>(1)?, "kind": r.get::<_, String>(2)?, "dataset": r.get::<_, String>(3)?,
                        "settings": j(4)?, "adapter_dir": r.get::<_, Option<String>>(5)?, "gguf_path": r.get::<_, Option<String>>(6)?, "status": r.get::<_, String>(7)?,
                        "scores": j(8)?, "regressions": j(9)?, "shadow": j(10)?, "checkpoint": r.get::<_, Option<String>>(11)?, "metrics": j(12)?,
                        "created_at": r.get::<_, String>(13)?, "updated_at": r.get::<_, String>(14)?}))
                },
            )
            .ok())
        })
        .ok()
        .flatten()
}

/// QLoRA-training via de worker (W13.02/W13.08): alleen in idle (P3); bij gebruik → pauzeren
/// (worker annuleren, checkpoint bewaren); later hervatten.
pub async fn train_adapter(
    chat: &ChatEngine,
    kind: &str,
    dataset: &str,
    resume: Option<String>,
) -> Result<Value, String> {
    let cfg = chat.train_cfg();
    let log = chat.kernel.events().clone();
    let path: String = log
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT path FROM training_datasets WHERE version = ?1", [dataset], |r| r.get(0)).ok())
        })
        .ok()
        .flatten()
        .ok_or_else(|| format!("dataset {dataset} onbekend"))?;
    let id = resume.clone().unwrap_or_else(|| format!("ad_{}", EventId::new().to_string().trim_start_matches("evt_")));
    let out = chat.training_dir().join("adapters").join(&id);
    let now = vortex_types::now().to_rfc3339();
    if resume.is_none() {
        log.db()
            .write(|tx| {
                tx.execute(
                    "INSERT INTO adapters(id, base_model, kind, dataset, settings, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'training', ?6, ?6)",
                    params![id, cfg.base_model, kind, dataset, Value::Object(cfg.lora.clone().into_iter().map(|(k, v)| (k, json!(v.to_string()))).collect()).to_string(), now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
    } else {
        adapter_set(&log, &id, &[("status", "training".into())]);
    }
    let mut spec = RunSpec::new(RunKind::Training, Priority::P3);
    spec.input = json!({"adapter": id, "kind": kind, "dataset": dataset});
    let ctx = chat.kernel.start_run(spec).map_err(|e| e.to_string())?;
    ctx.start().map_err(|e| e.to_string())?;
    let trace = ctx.trace().trace_id;
    let toml = format!(
        "kind = {kind:?}\nbase_model = {:?}\ndataset = {:?}\nout_dir = {:?}\nllama_cpp = {:?}\nresume = {}\n{}",
        cfg.base_model,
        path,
        out.display().to_string(),
        cfg.llama_cpp.clone().unwrap_or_default(),
        resume.is_some(),
        cfg.lora.iter().map(|(k, v)| format!("{k} = {v}\n")).collect::<String>()
    );
    let addr: String = chat
        .kernel
        .config()
        .current()
        .get::<String>("paths.worker_address")
        .ok()
        .flatten()
        .unwrap_or_else(|| "127.0.0.1:50061".into());
    let r = async {
        let mut lease = ctx.gpu(Priority::P3).await.map_err(|e| e.to_string())?;
        let ep = tonic::transport::Endpoint::from_shared(format!("http://{addr}")).map_err(|e| e.to_string())?.connect_timeout(Duration::from_secs(2));
        let ch = ep.connect().await.map_err(|_| format!("Python-worker niet bereikbaar op {addr}"))?;
        let mut client = WorkerServiceClient::new(ch.clone());
        let tc = TraceContext { trace_id: trace.to_string(), run_id: ctx.id().to_string(), ..Default::default() };
        let mut stream = client
            .train(TrainRequest { trace: Some(tc.clone()), job_kind: format!("lora:{kind}"), config_toml: toml, dataset: None })
            .await
            .map_err(|e| e.message().to_string())?
            .into_inner();
        let (mut result, mut metrics) = (None, HashMap::new());
        loop {
            let busy_user = async {
                // Gebruiker actief of iemand anders heeft de GPU nodig → pauzeren.
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if cfg.idle && !chat.kernel.dirigent().conditions().user_idle {
                        return;
                    }
                }
            };
            tokio::select! {
                _ = lease.preempted() => {
                    let _ = WorkerServiceClient::new(ch.clone()).cancel(CancelRequest { trace: Some(tc.clone()), ..Default::default() }).await;
                    return Ok::<_, String>(json!({"paused": true, "reason": "GPU nodig voor iets met hogere prioriteit"}));
                }
                _ = busy_user => {
                    let _ = WorkerServiceClient::new(ch.clone()).cancel(CancelRequest { trace: Some(tc.clone()), ..Default::default() }).await;
                    return Ok(json!({"paused": true, "reason": "gebruiker actief"}));
                }
                m = stream.message() => match m.map_err(|e| e.message().to_string())? {
                    Some(u) => {
                        for (k, v) in &u.metrics {
                            metrics.insert(k.clone(), *v);
                        }
                        match u.update {
                            Some(Update::Progress(p)) => {
                                let _ = log.append(Event::new(EventKind::TrainingProgress, trace, json!({"adapter": id, "fraction": p.fraction, "stage": p.stage, "message": p.message})).for_run(ctx.id()));
                            }
                            Some(Update::Artifact(a)) => result = Some((a.path, a.artifact_id)),
                            Some(Update::Error(e)) => return Err(e),
                            _ => {}
                        }
                    }
                    None => break,
                }
            }
        }
        let (gguf, adir) = result.ok_or("worker gaf geen adapter terug")?;
        Ok(json!({"gguf": gguf, "adapter_dir": adir, "metrics": metrics}))
    }
    .await;
    match &r {
        Ok(v) if v["paused"] == true => {
            adapter_set(
                &log,
                &id,
                &[("status", "paused".into()), ("checkpoint", out.join("checkpoints").display().to_string())],
            );
            let _ = ctx.finish_cancelled();
        }
        Ok(v) => {
            adapter_set(
                &log,
                &id,
                &[
                    ("status", "trained".into()),
                    ("gguf_path", v["gguf"].as_str().unwrap_or("").into()),
                    ("adapter_dir", v["adapter_dir"].as_str().unwrap_or("").into()),
                    ("metrics", v["metrics"].to_string()),
                ],
            );
            let _ = log.append(
                Event::new(EventKind::AdapterRegistered, trace, json!({"adapter": id, "gguf": v["gguf"]}))
                    .for_run(ctx.id()),
            );
            let _ = ctx.complete(v.clone());
        }
        Err(e) => {
            adapter_set(&log, &id, &[("status", "failed".into()), ("metrics", json!({"error": e}).to_string())]);
            let _ = ctx.fail(e);
        }
    }
    r.map(|mut v| {
        v["adapter"] = json!(id);
        v
    })
}

/// LoRA-ID in llama-server voor een adapter (via het GGUF-pad).
async fn lora_slot(
    chat: &ChatEngine,
    gguf: &str,
) -> Result<(std::sync::Arc<vortex_models::OpenAiBackend>, u32), String> {
    let cfg = chat.train_cfg();
    let reg = chat.registry().map_err(|e| e.to_string())?;
    let model = cfg.serve_model.clone().unwrap_or_else(|| reg.default.clone());
    let b = chat.backends.for_model(&reg, &model).map_err(|e| e.to_string())?;
    let list = b
        .lora_adapters()
        .await
        .map_err(|e| format!("llama-server /lora-adapters: {e} (adapter in models.toml onder loras zetten)"))?;
    let name = std::path::Path::new(gguf).file_name().and_then(|n| n.to_str()).unwrap_or(gguf).to_string();
    let id =
        list.iter().find(|(_, p, _)| p == gguf || p.ends_with(&name)).map(|(i, _, _)| *i).ok_or_else(|| {
            format!("adapter {name} is niet geladen in llama-server (voeg toe aan loras in models.toml)")
        })?;
    Ok((b, id))
}

/// Hot-swap (W13.04): adapter aan/uit zonder herstart.
pub async fn set_adapter_scale(chat: &ChatEngine, id: &str, scale: f32) -> Result<(), String> {
    let a = adapter(chat.kernel.events(), id).ok_or("adapter onbekend")?;
    let gguf = a["gguf_path"].as_str().ok_or("adapter heeft nog geen GGUF")?;
    let (b, slot) = lora_slot(chat, gguf).await?;
    b.set_lora(&[(slot, scale)]).await.map_err(|e| e.to_string())
}

/// Schaduwrun: recente echte vragen beantwoorden met en zonder adapter, Judge kiest blind.
pub async fn shadow_compare(state: &AppState, id: &str, n: usize) -> Result<Value, String> {
    let chat = &state.chat;
    let a = adapter(chat.kernel.events(), id).ok_or("adapter onbekend")?;
    let gguf = a["gguf_path"].as_str().ok_or("adapter heeft nog geen GGUF")?.to_string();
    let (b, slot) = lora_slot(chat, &gguf).await?;
    let qs: Vec<String> = db_rows(
        chat.kernel.events(),
        "SELECT NULL, content FROM messages WHERE role = 'user' ORDER BY created_at DESC LIMIT ?1",
        &[&((n * 2) as i64)],
    )
    .into_iter()
    .map(|(_, c)| c)
    .filter(|c| c.len() > 10)
    .take(n)
    .collect();
    let judge = chat.role_llm(&chat.kernel.config().current(), "chat", CancellationToken::new()).ok_or("geen model")?;
    let (mut wins, mut total) = (0usize, 0usize);
    for (i, q) in qs.iter().enumerate() {
        let ask = |lora: Option<Vec<(u32, f32)>>| {
            let b = b.clone();
            let q = q.clone();
            async move {
                let mut req = vortex_models::backend::ChatRequest::new(vec![vortex_models::backend::Message::user(q)]);
                req.lora = lora;
                use vortex_models::backend::Backend;
                b.chat(&req, &CancellationToken::new()).await.map(|r| r.content).unwrap_or_default()
            }
        };
        let base = ask(Some(vec![(slot, 0.0)])).await;
        let with = ask(Some(vec![(slot, 1.0)])).await;
        // Blind: volgorde wisselen.
        let (x, y, adapter_is_x) = if i % 2 == 0 { (&with, &base, true) } else { (&base, &with, false) };
        let out = judge
            .complete(
                vec![
                    vortex_models::backend::Message::system("Welk antwoord is beter (juister, nuttiger, duidelijker)? Antwoord als JSON: {\"best\": \"A\"|\"B\"}."),
                    vortex_models::backend::Message::user(format!("Vraag: {q}\n\nA:\n{x}\n\nB:\n{y}")),
                ],
                vortex_cognition::GenOpts { max_tokens: Some(20), temperature: Some(0.0), json_schema: None, reasoning_effort: None },
            )
            .await
            .unwrap_or_default();
        let best_a = vortex_agents::json::extract(&out).and_then(|v| v["best"].as_str().map(|s| s == "A"));
        if let Some(ba) = best_a {
            total += 1;
            if ba == adapter_is_x {
                wins += 1;
            }
        }
    }
    let res = json!({"n": total, "wins": wins, "win_rate": if total > 0 { wins as f64 / total as f64 } else { 0.0 }});
    adapter_set(chat.kernel.events(), id, &[("shadow", res.to_string())]);
    Ok(res)
}

/// Poortwachter (W13.07): eval met adapter vs laatste basis-run per suite; geen regressie >
/// max_regression, ergens ≥ min_gain winst, en (als er een schaduwrun is) genoeg gewonnen.
pub async fn gate(state: &AppState, id: &str) -> Result<Value, String> {
    let chat = &state.chat;
    let cfg = chat.train_cfg().gate;
    let log = chat.kernel.events().clone();
    let base: HashMap<String, f64> = db_rows(&log, "SELECT suite, CAST(score AS TEXT) FROM eval_runs WHERE score IS NOT NULL AND (label IS NULL OR label NOT LIKE 'adapter:%') ORDER BY started_at", &[])
        .into_iter()
        .filter_map(|(s, sc)| s.zip(sc.parse().ok()))
        .collect();
    set_adapter_scale(chat, id, 1.0).await?;
    // Geen eval-sets of geen basislijn → niets aantoonbaar → afwijzen (veilig).
    let sets = vortex_eval::load_dir(&crate::eval_api::evals_dir().join("sets")).unwrap_or_default();
    let subject = crate::eval_api::VortexSubject { state: state.clone(), mode_override: None };
    let runner = vortex_eval::Runner { log: &log, subject: &subject, python: crate::eval_api::python() };
    let mut scores = serde_json::Map::new();
    let mut regressions = Vec::new();
    let mut gains = 0;
    for suite in sets.iter().filter(|s| base.contains_key(&s.name)) {
        let r = runner.run_suite(suite, Some(&format!("adapter:{id}"))).await;
        let Ok(r) = r else { continue };
        let Some(sc) = r.score else { continue };
        let b = base[&suite.name];
        scores.insert(suite.name.clone(), json!({"adapter": sc, "base": b}));
        if sc < b - cfg.max_regression {
            regressions.push(json!({"suite": suite.name, "base": b, "adapter": sc}));
        }
        if sc >= b + cfg.min_gain {
            gains += 1;
        }
    }
    let shadow: Value = adapter(&log, id).map(|a| a["shadow"].clone()).unwrap_or(Value::Null);
    let shadow_ok = shadow["n"].as_u64().is_none_or(|n| (n as usize) < cfg.shadow_min)
        || shadow["win_rate"].as_f64().unwrap_or(0.0) >= cfg.shadow_win_rate;
    let ok = regressions.is_empty() && gains > 0 && shadow_ok;
    // Promoveren = adapter blijft aan; anders terug naar het basismodel.
    if !ok {
        let _ = set_adapter_scale(chat, id, 0.0).await;
    }
    let status = if ok { "active" } else { "rejected" };
    adapter_set(
        &log,
        id,
        &[
            ("status", status.into()),
            ("scores", Value::Object(scores.clone()).to_string()),
            ("regressions", Value::Array(regressions.clone()).to_string()),
        ],
    );
    let _ = log.append(Event::new(
        EventKind::AdapterDecided,
        TraceId::new(),
        json!({"adapter": id, "status": status, "gains": gains, "regressions": regressions, "shadow": shadow}),
    ));
    Ok(json!({"status": status, "scores": scores, "regressions": regressions, "gains": gains, "shadow": shadow}))
}

/// Kalibratie (W13.06): Judge-zekerheid vs werkelijke uitkomst (feedback per run), dev/test gescheiden.
pub fn calibrate(chat: &ChatEngine) -> Result<Calibrator, String> {
    let log = chat.kernel.events();
    let fb = feedback_by_run(log);
    let mut data = Vec::new();
    for (run, p) in db_rows(log, "SELECT run_id, payload FROM events WHERE kind = 'agent.judge.verdict'", &[]) {
        let Some(r) = run.and_then(|r| fb.get(&r).cloned()) else { continue };
        let v: Value = serde_json::from_str(&p).unwrap_or_default();
        if let Some(c) = v["verdict"]["confidence"].as_f64() {
            data.push((c, if r == "good" { 1.0 } else { 0.0 }));
        }
    }
    if data.len() < 10 {
        return Err(format!("te weinig gelabelde oordelen ({}); minimaal 10", data.len()));
    }
    let c = Calibrator::fit(&data);
    c.save(&chat.training_dir().join("calibrator.json")).map_err(|e| e.to_string())?;
    Ok(c)
}

/// Idle-vliegwiel: genoeg nieuwe voorbeelden → datasets bouwen → trainen; gepauzeerde
/// adapters hervatten zodra het weer stil is.
pub fn spawn_flywheel(state: AppState, stop: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(1800)) => {}
            }
            let chat = &state.chat;
            let cfg = chat.train_cfg();
            if !crate::eval_api::FEATURES.contains(&"W13")
                || cfg.base_model.is_empty()
                || !chat.kernel.dirigent().conditions().user_idle
            {
                continue;
            }
            // Gepauzeerde training hervatten.
            let paused: Vec<(Option<String>, String)> = db_rows(
                chat.kernel.events(),
                "SELECT id, kind || char(30) || dataset FROM adapters WHERE status = 'paused' LIMIT 1",
                &[],
            );
            if let Some((Some(id), kd)) = paused.into_iter().next() {
                let (k, d) = kd.split_once('\u{1e}').unwrap_or(("sft", ""));
                let _ = train_adapter(chat, k, d, Some(id)).await;
                continue;
            }
            let Ok(ds) = build_sft(chat) else { continue };
            let last: i64 = chat
                .kernel
                .events()
                .db()
                .read(|c| {
                    Ok(c.query_row(
                        "SELECT COALESCE(MAX(n), 0) FROM training_datasets d JOIN adapters a ON a.dataset = d.version",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap_or(0);
            if ds["n"].as_i64().unwrap_or(0) >= last + cfg.min_new_examples as i64
                && let Some(v) = ds["version"].as_str()
            {
                match train_adapter(chat, "sft", v, None).await {
                    Ok(r) => tracing::info!(?r, "adapter getraind"),
                    Err(e) => tracing::warn!(error = %e, "training mislukt"),
                }
            }
        }
    });
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/training", get(status))
        .route("/api/training/datasets/:kind", post(build))
        .route("/api/training/train", post(train))
        .route("/api/training/calibrate", post(calib))
        .route("/api/training/adapters/:id/gate", post(gate_h))
        .route("/api/training/adapters/:id/shadow", post(shadow_h))
        .route("/api/training/adapters/:id/scale", post(scale_h))
}

fn e400(e: String) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e)
}

async fn status(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let log = s.kernel.events();
    let datasets: Vec<Value> = log
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT version, kind, n, dropped, stats, created_at FROM training_datasets ORDER BY created_at DESC LIMIT 50")?;
            let v = st
                .query_map([], |r| {
                    Ok(json!({"version": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "n": r.get::<_, i64>(2)?, "decontaminated": r.get::<_, i64>(3)?, "stats": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null), "created_at": r.get::<_, String>(5)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap_or_default();
    let ids: Vec<String> = db_rows(log, "SELECT NULL, id FROM adapters ORDER BY created_at DESC LIMIT 50", &[])
        .into_iter()
        .map(|(_, i)| i)
        .collect();
    let adapters: Vec<Value> = ids.iter().filter_map(|i| adapter(log, i)).collect();
    let cal = Calibrator::load(&s.chat.training_dir().join("calibrator.json"));
    let cfg = s.chat.train_cfg();
    Ok(Json(
        json!({"datasets": datasets, "adapters": adapters, "calibrator": cal, "config": {"base_model": cfg.base_model, "idle": cfg.idle, "min_new_examples": cfg.min_new_examples}}),
    ))
}

async fn build(State(s): State<AppState>, AxPath(kind): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let r = match kind.as_str() {
        "sft" => build_sft(&s.chat),
        "dpo" => build_dpo(&s.chat),
        "qa" => {
            let cfg = s.kernel.config().current();
            let tok = s.kernel.shutdown_token().child_token();
            let llm = s.chat.role_llm(&cfg, "chat", tok).ok_or_else(|| e400("geen model".into()))?;
            build_qa(&s.chat, 200, llm).await
        }
        k => Err(format!("onbekende datasetsoort {k}")),
    };
    Ok(Json(r.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct TrainBody {
    kind: String,
    dataset: String,
}

async fn train(State(s): State<AppState>, Json(b): Json<TrainBody>) -> Result<Json<Value>, ApiError> {
    Ok(Json(train_adapter(&s.chat, &b.kind, &b.dataset, None).await.map_err(e400)?))
}

async fn calib(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(calibrate(&s.chat).map_err(e400)?)))
}

async fn gate_h(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(gate(&s, &id).await.map_err(e400)?))
}

async fn shadow_h(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(shadow_compare(&s, &id, 10).await.map_err(e400)?))
}

#[derive(Debug, Deserialize)]
struct ScaleBody {
    scale: f32,
}

async fn scale_h(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<ScaleBody>,
) -> Result<Json<Value>, ApiError> {
    set_adapter_scale(&s.chat, &id, b.scale).await.map_err(e400)?;
    let st = if b.scale > 0.0 { "active" } else { "trained" };
    adapter_set(s.kernel.events(), &id, &[("status", st.into())]);
    Ok(Json(json!({"ok": true, "status": st})))
}
