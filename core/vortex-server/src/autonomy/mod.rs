//! Autonomie (W15): taakqueue + triggers + executor (alles als runs via de Dirigent), leeragenda
//! in idle, proactieve monitoring met suggesties, dagelijkse briefing, backups met restore-test,
//! GPU-bewaker (vrijgeven bij gamen), OpenAI-compatibele API, voortgangsdashboard.
// VX:W15.01

pub mod api;
pub mod backup;
pub mod briefing;
pub mod cron;
pub mod gpu;
pub mod monitor;
pub mod openai;
pub mod progress;
pub mod queue;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vortex_core::RunSpec;
use vortex_types::{Priority, RunKind};

pub use queue::{NewTask, Queue, Task, Trigger, Triggers};

use crate::api::AppState;
use crate::chat::Mode;

/// Gelijktijdige taken van de executor.
const MAX_RUNNING: usize = 2;

#[derive(Debug, Clone)]
pub struct Autonomy {
    pub queue: Queue,
    pub triggers: Triggers,
    running: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
    pub gpu: Arc<gpu::GpuGuard>,
}

impl Autonomy {
    pub fn new(log: vortex_core::EventLog, gpu: Arc<gpu::GpuGuard>) -> Self {
        Self {
            queue: Queue::new(log.clone()),
            triggers: Triggers::new(log),
            running: Arc::new(Mutex::new(HashMap::new())),
            gpu,
        }
    }

    /// Standaardtriggers (één keer aangemaakt, uit te zetten in de GUI).
    pub fn ensure_builtin(&self) {
        let t = |title: &str, kind: &str, prio: i64, idle: bool| NewTask {
            title: title.into(),
            kind: kind.into(),
            priority: prio,
            needs_idle: Some(idle),
            max_attempts: Some(2),
            ..Default::default()
        };
        let _ =
            self.triggers.add("briefing", "cron", "30 7 * * *", &t("Dagelijkse briefing", "briefing", 2, false), true);
        let _ = self.triggers.add(
            "backup",
            "cron",
            "15 3 * * *",
            &t("Automatische backup + restore-test", "backup", 2, false),
            true,
        );
        let _ = self.triggers.add("leeragenda", "interval", "240", &t("Leeragenda uitvoeren", "agenda", 3, true), true);
        let _ = self.triggers.add("monitor", "interval", "30", &t("Proactieve monitoring", "monitor", 2, false), true);
        let _ = self.triggers.add(
            "eval-maand",
            "cron",
            "0 2 1 * *",
            &t("Maandelijkse eval (voortgang)", "eval", 3, true),
            true,
        );
    }

    pub fn cancel(&self, id: &str) -> bool {
        let ok = self.queue.cancel(id);
        if let Some(h) = self.running.lock().remove(id) {
            h.abort();
        }
        ok
    }

    pub fn pause(&self, id: &str) -> bool {
        let ok = self.queue.pause(id);
        if let Some(h) = self.running.lock().remove(id) {
            h.abort();
        }
        ok
    }
}

fn prio(p: i64) -> Priority {
    match p {
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    }
}

/// Executor: triggers afvuren, uitvoerbare taken starten (max. [`MAX_RUNNING`]).
pub fn spawn(state: AppState, stop: CancellationToken) {
    let a = state.autonomy.clone();
    let n = a.queue.recover();
    if n > 0 {
        tracing::info!(hervat = n, "taken hervat na herstart");
    }
    a.ensure_builtin();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
            if enabled(&state) {
                tick(&state);
            }
        }
    });
}

/// Executor aan? (`config/autonomy.toml`: `enabled = true`; zonder config uit.)
pub fn enabled(state: &AppState) -> bool {
    state.kernel.config().current().get::<bool>("autonomy.enabled").ok().flatten().unwrap_or(false)
}

/// Eén ronde (ook los aan te roepen in tests).
pub fn tick(state: &AppState) {
    let a = &state.autonomy;
    a.triggers.tick(&a.queue);
    let idle = state.kernel.dirigent().conditions().user_idle;
    loop {
        let busy: Vec<String> = {
            let mut g = a.running.lock();
            g.retain(|_, h| !h.is_finished());
            g.keys().cloned().collect()
        };
        if busy.len() >= MAX_RUNNING {
            return;
        }
        let Some(t) = a.queue.next_runnable(idle, &busy) else { return };
        a.queue.start(&t.id, None);
        let st = state.clone();
        let id = t.id.clone();
        let h = tokio::spawn(async move {
            let r = execute(&st, &t).await;
            if let (Some(p), Ok(v)) = (&t.project_id, &r) {
                st.autonomy.queue.note(p, &format!("{} klaar: {}", t.title, short(v)));
            }
            st.autonomy.queue.finish(&t.id, r);
        });
        a.running.lock().insert(id, h.abort_handle());
    }
}

fn short(v: &Value) -> String {
    let s = v
        .get("summary")
        .or(v.get("answer"))
        .and_then(|x| x.as_str())
        .map(String::from)
        .unwrap_or_else(|| v.to_string());
    s.chars().take(160).collect()
}

/// Taak uitvoeren. Soorten die zelf een run starten (world, dream, train_layer, ingest, prompt,
/// eval) doen dat via hun eigen pad; de rest krijgt hier een run (zichtbaar in Lopend werk).
pub async fn execute(state: &AppState, t: &Task) -> Result<Value, String> {
    let chat = &state.chat;
    let p = &t.params;
    let s = |k: &str| p[k].as_str().map(String::from);
    match t.kind.as_str() {
        "prompt" => {
            let q = s("prompt").ok_or("params.prompt ontbreekt")?;
            let ws = s("workspace").or_else(|| {
                t.project_id
                    .as_ref()
                    .and_then(|pid| state.autonomy.queue.project(pid))
                    .and_then(|p| p["workspace"].as_str().map(String::from))
            });
            let a = chat
                .answer_once(&q, Mode::parse(&s("mode").unwrap_or_default()), ws)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({"answer": a}))
        }
        "world" => crate::world_api::cycle(chat, None).await,
        "dream" => chat.dream_now(prio(t.priority)).await,
        "train_layer" => {
            crate::neural_api::train_layer(chat, &s("layer").ok_or("params.layer ontbreekt")?, prio(t.priority)).await
        }
        "ingest" => {
            let paths: Vec<String> =
                p["paths"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
            let (run, rep) = chat.ingest(paths, s("workspace"), prio(t.priority)).await?;
            Ok(json!({"run": run, "report": rep}))
        }
        "eval" => crate::eval_api::run_suites(state, s("suite").as_deref(), s("mode")).await,
        // W18.06: verbeterde systeemprompt toepassen (na A/B-winst + akkoord van de gebruiker).
        "apply_prompt" => {
            crate::api::set_prompt(state, "prompts/system.md", &s("content").ok_or("params.content ontbreekt")?)
                .map_err(|e| format!("{e:?}"))
        }
        "improve_tools" => crate::selfimprove::improve_tools(state).await,
        kind => {
            let mut spec = RunSpec::new(RunKind::Other(format!("task:{kind}")), prio(t.priority));
            spec.input = json!({"task": t.id, "title": t.title, "params": p});
            let ctx = chat.kernel.start_run(spec).map_err(|e| e.to_string())?;
            ctx.start().map_err(|e| e.to_string())?;
            let r = match kind {
                "research" => {
                    let _gpu = ctx.gpu(prio(t.priority)).await.map_err(|e| e.to_string())?;
                    let q = s("question").ok_or("params.question ontbreekt")?;
                    let rid = format!("res_{}", t.id.trim_start_matches("task_"));
                    let mode = vortex_agents::research::ResearchMode::parse(&s("mode").unwrap_or_default());
                    let o = chat.research_in(&ctx, &rid, &q, mode, s("workspace"), None).await;
                    Ok(
                        json!({"research": rid, "sources": o.sources.len(), "records": o.records, "summary": o.report.chars().take(400).collect::<String>()}),
                    )
                }
                "agenda" => run_agenda(state, &ctx, p["max"].as_u64().unwrap_or(2) as usize, prio(t.priority)).await,
                "briefing" => briefing::write(state, None).await,
                "backup" => backup::run(state).await,
                "monitor" => Ok(monitor::scan(state).await),
                k => Err(format!("onbekende taaksoort {k}")),
            };
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
}

/// W15.02: leeragenda zelf uitvoeren — gaten en betwiste feiten onderzoeken (research; online
/// als er verbinding is, anders lokaal), zwakke taaktypen → eval + (via het vliegwiel) training.
// VX:W15.02
async fn run_agenda(state: &AppState, ctx: &vortex_core::RunContext, max: usize, p: Priority) -> Result<Value, String> {
    let chat = &state.chat;
    let items = chat.agenda().list(Some("open"), max.max(1)).map_err(|e| e.to_string())?;
    let mut done = Vec::new();
    for it in items {
        let _ = chat.agenda().set_status(&it.id, "researching");
        let r = match it.reason.as_str() {
            "weak_task" => {
                let t = state.autonomy.queue.enqueue(NewTask {
                    title: format!("Eval na zwak taaktype {}", it.topic),
                    kind: "eval".into(),
                    priority: 3,
                    origin: Some(format!("agenda:{}", it.id)),
                    ..Default::default()
                })?;
                json!({"topic": it.topic, "action": "eval", "task": t.id})
            }
            _ => {
                let q = match it.reason.as_str() {
                    "disputed" => {
                        format!("Wat is correct over: {}? Er zijn tegenstrijdige bronnen ({}).", it.topic, it.detail)
                    }
                    _ => format!("Wat moet ik weten over {}? ({})", it.topic, it.detail),
                };
                let _gpu = ctx.gpu(p).await.map_err(|e| e.to_string())?;
                let rid =
                    format!("res_ag_{}", it.id.chars().filter(|c| c.is_alphanumeric()).take(16).collect::<String>());
                let mode = vortex_agents::research::ResearchMode::Quick;
                let o = chat.research_in(ctx, &rid, &q, mode, None, None).await;
                json!({"topic": it.topic, "action": "research", "research": rid, "sources": o.sources.len(), "records": o.records})
            }
        };
        let _ = chat.agenda().set_status(&it.id, "done");
        done.push(r);
    }
    Ok(json!({"done": done, "summary": format!("{} agenda-item(s) afgehandeld", done.len())}))
}

/// Tailscale-adres van deze PC (`tailscale ip -4`), als Tailscale draait.
// VX:W15.08
pub fn tailscale_ip() -> Option<String> {
    let out = std::process::Command::new("tailscale").args(["ip", "-4"]).output().ok()?;
    let ip = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    (out.status.success() && ip.parse::<std::net::Ipv4Addr>().is_ok()).then_some(ip)
}

/// Telefoontoegang: aan in config? welk adres?
pub fn remote_info(state: &AppState) -> Value {
    let on = state.kernel.config().current().get::<bool>("autonomy.remote.tailscale").ok().flatten().unwrap_or(false);
    let ip = if on { tailscale_ip() } else { None };
    json!({"tailscale": on, "ip": ip, "url": ip.as_ref().map(|i| format!("http://{i}:7420"))})
}
