//! Research + ingest in de server (W10): research-runs (chat én Research-pagina), ingest-API,
//! records-export (CSV/JSONL/Parquet), globale corpus-synthese, documenten uit crawls → ingest.
// VX:W10.11

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use vortex_agents::research::{ResearchDeps, ResearchMode, ResearchOutcome};
use vortex_core::{ConfigSnapshot, RunContext, RunSpec};
use vortex_crawler::{CrawlConfig, Crawler};
use vortex_types::{EventId, Priority, RunKind};

use crate::api::{ApiError, AppState};
use crate::chat::ChatEngine;

/// Koude schijf (ruwe data) uit `paths.cold_dir`, anders `<data>/cold`.
pub fn cold_dir(cfg: &ConfigSnapshot, data_dir: &std::path::Path) -> PathBuf {
    cfg.get::<String>("paths.cold_dir")
        .ok()
        .flatten()
        .map(PathBuf::from)
        .filter(|p| p.parent().is_some_and(|q| q.exists()))
        .unwrap_or_else(|| data_dir.join("cold"))
}

impl ChatEngine {
    pub(crate) fn data_dir(&self) -> PathBuf {
        self.kernel.db().path().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_else(std::env::temp_dir)
    }

    pub(crate) fn crawler(&self) -> Arc<Crawler> {
        let mut g = self.crawler.lock();
        if let Some(c) = g.as_ref() {
            return c.clone();
        }
        let cfg = CrawlConfig {
            raw_dir: Some(cold_dir(&self.kernel.config().current(), &self.data_dir()).join("crawl")),
            ..Default::default()
        };
        let c = Arc::new(Crawler::new(self.kernel.events().clone(), cfg));
        *g = Some(c.clone());
        c
    }

    /// Research binnen een bestaande run (chat) of als eigen run (Research-pagina).
    pub(crate) async fn research_in(
        &self,
        ctx: &RunContext,
        research_id: &str,
        question: &str,
        mode: ResearchMode,
        workspace: Option<String>,
        ws: Option<vortex_cognition::Workspace>,
    ) -> ResearchOutcome {
        let (run, trace) = (ctx.id(), ctx.trace().trace_id);
        let reg = vortex_models::Registry::from_config(ctx.config()).ok();
        let main = self.role_llm(ctx.config(), "chat", ctx.token().child_token());
        let deps = ResearchDeps {
            main: match main {
                Some(m) => m,
                None => Arc::new(NoModel),
            },
            aux: reg.and(self.role_llm(ctx.config(), "extract", ctx.token().child_token())),
            memory: self.memory.as_ref().map(|m| (m.store.clone(), m.reranker.clone())),
            mem_workspace: workspace.clone(),
            tools: self.tools.clone(),
            crawler: Some(self.crawler()),
            online: self.online.online().await,
            reporter: vortex_agents::Reporter::new(Some(self.kernel.events().clone()), Some(run), trace),
            exec: vortex_tools::ExecOptions {
                run: Some(run),
                trace,
                cancel: ctx.token().child_token(),
                ..Default::default()
            },
            workspace: ws,
            log: self.kernel.events().clone(),
            research_id: research_id.to_string(),
            out_dir: Some(self.data_dir().join("reports")),
        };
        let out = vortex_agents::research::run(&deps, question, mode).await;
        // Gedownloade documenten (PDF's uit de crawl) op de achtergrond inlezen.
        if !out.documents.is_empty() {
            let me = self.clone();
            let docs = out.documents.clone();
            tokio::spawn(async move {
                let _ = me.ingest(docs, workspace, Priority::P3).await;
            });
        }
        out
    }

    /// Research als eigen run (P1; zichtbaar in Lopend werk).
    pub async fn start_research(
        &self,
        question: String,
        mode: ResearchMode,
        workspace: Option<String>,
    ) -> Result<String, String> {
        let mut spec = RunSpec::new(RunKind::Research, Priority::P1);
        spec.input = json!({"question": question, "mode": mode.as_str(), "workspace": workspace});
        let ctx = self.kernel.start_run(spec).map_err(|e| e.to_string())?;
        ctx.start().map_err(|e| e.to_string())?;
        let id = format!("res_{}", EventId::new().to_string().trim_start_matches("evt_"));
        let rid = id.clone();
        let me = self.clone();
        tokio::spawn(async move {
            let ws = vortex_cognition::Workspace::new(
                ctx.id(),
                ctx.trace().trace_id,
                &question,
                Some(me.kernel.events().clone()),
            );
            let r = async {
                let _gpu = ctx.gpu(Priority::P1).await.map_err(|e| e.to_string())?;
                Ok::<_, String>(me.research_in(&ctx, &rid, &question, mode, workspace, Some(ws)).await)
            }
            .await;
            match r {
                Ok(o) => {
                    let _ = ctx.complete(json!({"research": rid, "sources": o.sources.len(), "records": o.records}));
                }
                Err(e) => {
                    let _ = ctx.fail(&e);
                }
            }
        });
        Ok(id)
    }
}

/// Zonder model: research kan alleen eerlijk melden dat er geen model is.
#[derive(Debug)]
struct NoModel;
#[async_trait::async_trait]
impl vortex_cognition::CogLlm for NoModel {
    async fn complete(
        &self,
        _m: Vec<vortex_models::backend::Message>,
        _o: vortex_cognition::GenOpts,
    ) -> Result<String, String> {
        Err("geen model geconfigureerd".into())
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/research", get(list).post(start))
        .route("/api/research/corpus", post(corpus))
        .route("/api/research/:id", get(detail))
        .route("/api/research/:id/export", get(export))
        .route("/api/ingest", post(ingest))
        .route("/api/ingest/documents", get(documents))
}

#[derive(Debug, Deserialize)]
struct StartBody {
    question: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    workspace: Option<String>,
}

async fn start(State(s): State<AppState>, Json(b): Json<StartBody>) -> Result<Json<Value>, ApiError> {
    if b.question.trim().len() < 3 {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "vraag is te kort"));
    }
    let id = s
        .chat
        .start_research(b.question, ResearchMode::parse(b.mode.as_deref().unwrap_or("standard")), b.workspace)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"id": id})))
}

async fn list(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let v = s
        .kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, question, mode, status, created_at, updated_at, run_id FROM research_runs ORDER BY created_at DESC LIMIT 100")?;
            let v = st
                .query_map([], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "question": r.get::<_, String>(1)?, "mode": r.get::<_, String>(2)?, "status": r.get::<_, String>(3)?, "created_at": r.get::<_, String>(4)?, "updated_at": r.get::<_, String>(5)?, "run_id": r.get::<_, Option<String>>(6)?}))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(v)))
}

fn records(s: &AppState, id: &str) -> Result<Vec<Value>, ApiError> {
    s.kernel
        .events()
        .db()
        .read(|c| {
            let mut st = c.prepare("SELECT id, subquestion, kind, data, source_url, passage, status, memory_id, created_at FROM research_records WHERE research_id = ?1 ORDER BY created_at")?;
            let v = st
                .query_map([id], |r| {
                    let data: String = r.get(3)?;
                    Ok(json!({
                        "id": r.get::<_, String>(0)?, "subquestion": r.get::<_, Option<i64>>(1)?, "kind": r.get::<_, String>(2)?,
                        "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null), "source_url": r.get::<_, Option<String>>(4)?,
                        "passage": r.get::<_, Option<String>>(5)?, "status": r.get::<_, String>(6)?, "memory_id": r.get::<_, Option<String>>(7)?,
                        "created_at": r.get::<_, String>(8)?
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn detail(State(s): State<AppState>, AxPath(id): AxPath<String>) -> Result<Json<Value>, ApiError> {
    let run = s
        .kernel
        .events()
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT question, mode, status, plan, sources, report, report_path, run_id FROM research_runs WHERE id = ?1", [&id], |r| {
                Ok(json!({
                    "question": r.get::<_, String>(0)?, "mode": r.get::<_, String>(1)?, "status": r.get::<_, String>(2)?,
                    "subquestions": serde_json::from_str::<Value>(&r.get::<_, Option<String>>(3)?.unwrap_or_default()).unwrap_or(json!([])),
                    "sources": serde_json::from_str::<Value>(&r.get::<_, Option<String>>(4)?.unwrap_or_default()).unwrap_or(json!([])),
                    "report": r.get::<_, Option<String>>(5)?, "report_path": r.get::<_, Option<String>>(6)?, "run_id": r.get::<_, Option<String>>(7)?
                }))
            })
            .ok())
        })
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "onderzoek onbekend"))?;
    let mut run = run;
    run["records"] = json!(records(&s, &id)?);
    run["id"] = json!(id);
    Ok(Json(run))
}

#[derive(Debug, Deserialize)]
struct ExportQ {
    #[serde(default)]
    format: Option<String>,
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// Records exporteren: CSV (platgeslagen), JSONL, of Parquet (via Python + pyarrow).
async fn export(
    State(s): State<AppState>,
    AxPath(id): AxPath<String>,
    Query(q): Query<ExportQ>,
) -> Result<Response, ApiError> {
    let recs = records(&s, &id)?;
    let fmt = q.format.unwrap_or_else(|| "csv".into());
    let flat: Vec<serde_json::Map<String, Value>> = recs
        .iter()
        .map(|r| {
            let mut m = serde_json::Map::new();
            for k in ["id", "kind", "status", "source_url", "subquestion", "passage"] {
                m.insert(k.into(), r[k].clone());
            }
            m.insert(
                "text".into(),
                json!(vortex_agents::records::render(r["kind"].as_str().unwrap_or(""), &r["data"])),
            );
            m.insert("data".into(), json!(r["data"].to_string()));
            m
        })
        .collect();
    match fmt.as_str() {
        "jsonl" => {
            let body: String = recs.iter().map(|r| format!("{r}\n")).collect();
            Ok((
                [
                    (header::CONTENT_TYPE, "application/x-ndjson"),
                    (header::CONTENT_DISPOSITION, "attachment; filename=records.jsonl"),
                ],
                body,
            )
                .into_response())
        }
        "parquet" => {
            let dir = s.chat.data_dir().join("exports");
            std::fs::create_dir_all(&dir)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let src = dir.join(format!("{id}.jsonl"));
            let dst = dir.join(format!("{id}.parquet"));
            let lines: String = flat.iter().map(|m| format!("{}\n", Value::Object(m.clone()))).collect();
            std::fs::write(&src, lines).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let code = format!(
                "import json,sys\ntry:\n import pyarrow as pa, pyarrow.parquet as pq\nexcept ImportError:\n print('pyarrow ontbreekt'); sys.exit(3)\nrows=[json.loads(l) for l in open({:?},encoding='utf-8')]\nrows=[{{k:(None if v is None else str(v)) for k,v in r.items()}} for r in rows]\npq.write_table(pa.Table.from_pylist(rows), {:?})\n",
                src.display().to_string(),
                dst.display().to_string()
            );
            let hub =
                s.tools.as_ref().ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "tools niet actief"))?;
            let r = hub
                .executor
                .call(&format!("export:{id}:{}", EventId::new()), "python", json!({"code": code}), &Default::default())
                .await;
            if !r.ok {
                return Err(ApiError::new(
                    StatusCode::NOT_IMPLEMENTED,
                    format!(
                        "Parquet-export: {} (installeer pyarrow: `uv pip install pyarrow` of `pip install pyarrow`)",
                        r.for_model()
                    ),
                ));
            }
            let bytes =
                std::fs::read(&dst).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((
                [
                    (header::CONTENT_TYPE, "application/vnd.apache.parquet"),
                    (header::CONTENT_DISPOSITION, "attachment; filename=records.parquet"),
                ],
                bytes,
            )
                .into_response())
        }
        _ => {
            let cols = ["id", "kind", "text", "status", "source_url", "subquestion", "passage", "data"];
            let mut out = cols.join(",");
            out.push('\n');
            for m in &flat {
                let row: Vec<String> = cols
                    .iter()
                    .map(|c| {
                        csv_escape(
                            &m.get(*c)
                                .map(|v| {
                                    v.as_str()
                                        .map(String::from)
                                        .unwrap_or_else(|| if v.is_null() { String::new() } else { v.to_string() })
                                })
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                out.push_str(&row.join(","));
                out.push('\n');
            }
            Ok((
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (header::CONTENT_DISPOSITION, "attachment; filename=records.csv"),
                ],
                out,
            )
                .into_response())
        }
    }
}

#[derive(Debug, Deserialize)]
struct CorpusBody {
    question: String,
    #[serde(default)]
    workspace: Option<String>,
}

async fn corpus(State(s): State<AppState>, Json(b): Json<CorpusBody>) -> Result<Json<Value>, ApiError> {
    let cfg = s.kernel.config().current();
    let token = s.kernel.shutdown_token().child_token();
    let main = s
        .chat
        .role_llm(&cfg, "chat", token.clone())
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "geen model"))?;
    let aux = s.chat.role_llm(&cfg, "extract", token);
    let text = vortex_agents::research::corpus_synthesis(
        &main,
        aux.as_ref(),
        &s.memory.store,
        b.workspace.as_deref(),
        &b.question,
    )
    .await;
    Ok(Json(json!({"answer": text})))
}

#[derive(Debug, Deserialize)]
struct IngestBody {
    paths: Vec<String>,
    #[serde(default)]
    workspace: Option<String>,
}

async fn ingest(State(s): State<AppState>, Json(b): Json<IngestBody>) -> Result<Json<Value>, ApiError> {
    if b.paths.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "geen paden"));
    }
    let chat = s.chat.clone();
    // Achtergrond (P2); voortgang via ingest.job.*-events.
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let r = chat.ingest(b.paths, b.workspace, Priority::P2).await;
        let _ = tx.send(r);
    });
    // Klein werk is meteen klaar; anders melden we "gestart".
    match tokio::time::timeout(std::time::Duration::from_secs(2), rx).await {
        Ok(Ok(Ok((run, rep)))) => Ok(Json(json!({"run_id": run, "report": rep, "done": true}))),
        Ok(Ok(Err(e))) => Err(ApiError::new(StatusCode::BAD_REQUEST, e)),
        _ => Ok(Json(json!({"done": false}))),
    }
}

async fn documents(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        s.memory.store.documents(None).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    )))
}
