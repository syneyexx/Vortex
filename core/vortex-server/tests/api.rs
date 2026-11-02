//! W03 server-tests: chat end-to-end tegen een nep-model, hervatten van de eventstream,
//! stoppen midden in een stream, fork, instellingen met versies.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Json;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use vortex_core::{Kernel, KernelOptions, SecretBroker};

/// Nep-llama-server: streamt "Hallo wereld" in stukjes; "slow" streamt lang.
async fn fake_model() -> String {
    async fn chat(Json(b): Json<Value>) -> Response {
        let last = b["messages"]
            .as_array()
            .and_then(|m| m.last())
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .to_string();
        if !b["stream"].as_bool().unwrap_or(false) {
            let schema = b["response_format"]["json_schema"]["schema"]["properties"].clone();
            let content = if schema.get("steps").is_some() {
                json!({"steps": ["feiten verzamelen", "vergelijken"]}).to_string()
            } else if schema.get("confidence").is_some() {
                json!({"verdict": "accept", "confidence": 0.85, "reasons": ["onderbouwd"]}).to_string()
            } else if schema.get("verdict").is_some() {
                json!({"verdict": "revise", "issues": ["mist een bron"]}).to_string()
            } else {
                "stapresultaat".to_string()
            };
            return axum::Json(json!({"choices":[{"message":{"content": content},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}})).into_response();
        }
        let last = if last.starts_with("Bijsturing van de gebruiker") { "na-bijsturing".to_string() } else { last };
        // Toolflow (W08): "TOOLTEST <pad>" → model vraagt write_file + read_file; na toolresultaten → eindantwoord.
        let last_role =
            b["messages"].as_array().and_then(|m| m.last()).and_then(|m| m["role"].as_str()).unwrap_or("").to_string();
        let has_tools = b["tools"].as_array().is_some_and(|t| !t.is_empty());
        if let (Some(path), true, false) = (last.strip_prefix("TOOLTEST "), has_tools, last_role == "tool") {
            let calls = json!([
                {"index": 0, "id": "c1", "type": "function", "function": {"name": "write_file", "arguments": json!({"path": path, "content": "hallo uit een tool\n"}).to_string()}},
                {"index": 1, "id": "c2", "type": "function", "function": {"name": "read_file", "arguments": json!({"path": path}).to_string()}}
            ]);
            let chunks: Vec<Vec<u8>> = vec![
                format!("data: {}\n\n", json!({"choices":[{"delta":{"content": "Ik schrijf het bestand. "}}]})).into_bytes(),
                format!("data: {}\n\n", json!({"choices":[{"delta":{"tool_calls": calls}}]})).into_bytes(),
                format!("data: {}\n\n", json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":3}})).into_bytes(),
                b"data: [DONE]\n\n".to_vec(),
            ];
            let s = futures::stream::iter(chunks.into_iter().map(Ok::<_, std::io::Error>));
            return Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(s))
                .unwrap_or_default();
        }
        let last = if last_role == "tool" {
            format!("Klaar, gelezen: {}", last.lines().last().unwrap_or("").trim())
        } else {
            last
        };
        let sys = b["messages"][0]["content"].as_str().unwrap_or("").to_string();
        let words: Vec<String> = if last == "slow" {
            (0..400).map(|i| format!("w{i} ")).collect()
        } else if last == "welke prompt?" {
            vec![sys]
        } else if last.starts_with("Klaar") {
            vec![last.clone()]
        } else {
            vec!["Hal".into(), "lo ".into(), "wereld".into()]
        };
        let delay = if last == "slow" { 10 } else { 5 };
        let mut chunks: Vec<Vec<u8>> = words
            .into_iter()
            .map(|w| format!("data: {}\n\n", json!({"choices":[{"delta":{"content": w}}]})).into_bytes())
            .collect();
        chunks.push(
            format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":5,"completion_tokens":3},"timings":{"predicted_per_second":42.0}})
            )
            .into_bytes(),
        );
        chunks.push(b"data: [DONE]\n\n".to_vec());
        let s = futures::stream::iter(chunks).then(move |c| async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            Ok::<_, std::io::Error>(c)
        });
        Response::builder().header("content-type", "text/event-stream").body(Body::from_stream(s)).unwrap_or_default()
    }
    let app = Router::new().route("/health", get(|| async { "ok" })).route("/v1/chat/completions", post(chat));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("{e}"));
    let addr = l.local_addr().unwrap_or_else(|e| panic!("{e}"));
    tokio::spawn(async move { axum::serve(l, app).await });
    format!("http://{addr}")
}

struct Fx {
    _dir: tempfile::TempDir,
    cfg: std::path::PathBuf,
    base: String,
    http: reqwest::Client,
    kernel: Kernel,
}

async fn setup() -> Fx {
    let model = fake_model().await;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let cfg = dir.path().join("config");
    std::fs::create_dir_all(cfg.join("prompts")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        cfg.join("models.toml"),
        format!("default = \"m\"\n[servers.main]\nurl = \"{model}\"\nmodel = \"m\"\n[models.m]\nfile = \"m.gguf\"\nroles = [\"chat\"]\ngpu = \"main\"\n"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(cfg.join("prompts/system.md"), "SYS-V1").unwrap_or_else(|e| panic!("{e}"));
    let mut o = KernelOptions::new(dir.path().join("data"), &cfg);
    o.secrets = SecretBroker::memory();
    o.watch_config = false;
    let (kernel, _) = Kernel::open(o).await.unwrap_or_else(|e| panic!("{e}"));
    let state = vortex_server::api::build_state(kernel.clone()).unwrap_or_else(|e| panic!("{e}"));
    let app = vortex_server::api::router(state, None);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("{e}"));
    let addr = l.local_addr().unwrap_or_else(|e| panic!("{e}"));
    tokio::spawn(async move { axum::serve(l, app).await });
    Fx { _dir: dir, cfg, base: format!("http://{addr}"), http: reqwest::Client::new(), kernel }
}

impl Fx {
    async fn post(&self, path: &str, body: Value) -> Value {
        self.http
            .post(format!("{}{path}", self.base))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .json()
            .await
            .unwrap_or_default()
    }
    async fn put(&self, path: &str, body: Value) -> Value {
        self.http
            .put(format!("{}{path}", self.base))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .json()
            .await
            .unwrap_or_default()
    }
    async fn get(&self, path: &str) -> Value {
        self.http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .json()
            .await
            .unwrap_or_default()
    }
    async fn wait_message(&self, sid: &str, idx: usize, status: &str) -> Value {
        for _ in 0..200 {
            let s = self.get(&format!("/api/sessions/{sid}")).await;
            if s["messages"][idx]["status"] == status {
                return s["messages"][idx].clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("bericht {idx} kreeg geen status {status}");
    }
}

#[tokio::test]
async fn chat_end_to_end_is_stored_and_logged() {
    let fx = setup().await;
    let s = fx.post("/api/sessions", json!({})).await;
    let sid = s["id"].as_str().unwrap_or_default().to_string();
    let r = fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "hoi", "mode": "quick"})).await;
    assert!(r["run_id"].is_string(), "{r}");
    let m = fx.wait_message(&sid, 1, "complete").await;
    assert_eq!(m["content"], "Hallo wereld");
    assert_eq!(m["mode"], "quick");
    assert_eq!(m["metrics"]["metrics"]["decode_tps"], 42.0);
    let sess = fx.get(&format!("/api/sessions/{sid}")).await;
    assert_eq!(sess["session"]["title"], "hoi", "titel uit eerste bericht");
    let runs = fx.get("/api/runs").await;
    assert_eq!(runs["recent"][0]["state"], "completed");
}

#[tokio::test]
async fn event_stream_resumes_from_seq_without_gaps() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "hoi"})).await;
    fx.wait_message(&sid, 1, "complete").await;
    let ws_url = fx.base.replace("http://", "ws://");
    // eerste verbinding: alles
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("{ws_url}/api/events?from_seq=0"))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut seqs = Vec::new();
    let mut text = String::new();
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(300), ws.next()).await {
        let e: Value = serde_json::from_str(msg.to_text().unwrap_or("{}")).unwrap_or_default();
        seqs.push(e["seq"].as_i64().unwrap_or(-1));
        if e["kind"] == "chat.message.delta" {
            text.push_str(e["payload"]["text"].as_str().unwrap_or_default());
        }
    }
    assert_eq!(text, "Hallo wereld", "tekst volledig te reconstrueren uit deltas");
    for w in seqs.windows(2) {
        assert_eq!(w[1], w[0] + 1, "geen gaten in volgnummers");
    }
    let _ = ws.close(None).await;
    // reconnect halverwege: alleen wat erna kwam
    let mid = seqs[seqs.len() / 2];
    let (mut ws2, _) = tokio_tungstenite::connect_async(format!("{ws_url}/api/events?from_seq={mid}"))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let first = ws2.next().await;
    let e: Value = first
        .and_then(|m| m.ok())
        .map(|m| serde_json::from_str(m.to_text().unwrap_or("{}")).unwrap_or_default())
        .unwrap_or_default();
    assert_eq!(e["seq"].as_i64(), Some(mid + 1));
    // live: nieuw bericht komt binnen op dezelfde verbinding
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "nog eens"})).await;
    let mut got_live = false;
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_secs(2), ws2.next()).await {
        let e: Value = serde_json::from_str(msg.to_text().unwrap_or("{}")).unwrap_or_default();
        if e["kind"] == "chat.message.completed" && e["payload"]["status"] == "complete" {
            let n = fx.get(&format!("/api/sessions/{sid}")).await["messages"].as_array().map(|a| a.len()).unwrap_or(0);
            if n == 4 {
                got_live = true;
                break;
            }
        }
    }
    assert!(got_live);
    let _ = ws2.send(tokio_tungstenite::tungstenite::Message::Close(None)).await;
}

#[tokio::test]
async fn stop_mid_stream_really_stops() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let r = fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "slow"})).await;
    let run = r["run_id"].as_str().unwrap_or_default().to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Herladen tijdens streamen: de GUI haalt de sessie opnieuw op en mag niets missen.
    let mid = fx.get(&format!("/api/sessions/{sid}")).await["messages"][1].clone();
    assert_eq!(mid["status"], "streaming");
    assert!(mid["content"].as_str().unwrap_or_default().starts_with("w0 w1 "), "deeltekst uit event log: {mid}");
    let t0 = std::time::Instant::now();
    let st = fx.post(&format!("/api/runs/{run}/stop"), json!({})).await;
    assert_eq!(st["accepted"], true);
    let m = fx.wait_message(&sid, 1, "cancelled").await;
    assert!(t0.elapsed() < Duration::from_millis(1500));
    let len = m["content"].as_str().unwrap_or_default().len();
    assert!(len > 0 && len < 400 * 4, "deels gestreamd en daarna gestopt ({len})");
    let rec = fx.kernel.runs().get(run.parse().unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rec.state, vortex_types::RunState::Cancelled);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after =
        fx.get(&format!("/api/sessions/{sid}")).await["messages"][1]["content"].as_str().unwrap_or_default().len();
    assert_eq!(after, len, "na stop komt er niets meer bij");
}

#[tokio::test]
async fn closing_the_connection_does_not_stop_the_run() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let ws_url = fx.base.replace("http://", "ws://");
    let (ws, _) =
        tokio_tungstenite::connect_async(format!("{ws_url}/api/events")).await.unwrap_or_else(|e| panic!("{e}"));
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "hoi"})).await;
    drop(ws); // GUI weg
    let m = fx.wait_message(&sid, 1, "complete").await;
    assert_eq!(m["content"], "Hallo wereld");
}

#[tokio::test]
async fn fork_copies_history_up_to_message() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "een"})).await;
    fx.wait_message(&sid, 1, "complete").await;
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "twee"})).await;
    fx.wait_message(&sid, 3, "complete").await;
    let msgs = fx.get(&format!("/api/sessions/{sid}")).await["messages"].clone();
    let at = msgs[1]["id"].as_str().unwrap_or_default();
    let f = fx.post(&format!("/api/sessions/{sid}/fork"), json!({"message_id": at})).await;
    let fid = f["id"].as_str().unwrap_or_default();
    let fm = fx.get(&format!("/api/sessions/{fid}")).await["messages"].clone();
    assert_eq!(fm.as_array().map(|a| a.len()), Some(2));
    assert_eq!(f["parent_session_id"].as_str(), Some(sid.as_str()));
}

#[tokio::test]
async fn system_prompt_is_live_versioned_and_restorable() {
    let fx = setup().await;
    let p = fx.get("/api/settings/prompts").await;
    let v1 = p["version"].as_str().unwrap_or_default().to_string();
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "welke prompt?"})).await;
    let c1 = fx.wait_message(&sid, 1, "complete").await["content"].as_str().unwrap_or_default().to_string();
    assert!(c1.starts_with("SYS-V1"), "{c1}");
    let r = fx.put("/api/settings/prompts", json!({"path": "prompts/system.md", "content": "SYS-V2"})).await;
    assert_ne!(r["version"].as_str(), Some(v1.as_str()));
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "welke prompt?"})).await;
    let c = fx.wait_message(&sid, 3, "complete").await["content"].as_str().unwrap_or_default().to_string();
    assert!(c.starts_with("SYS-V2"), "live vanaf volgend bericht: {c}");
    let hist = fx.get("/api/settings/history").await;
    assert!(hist.as_array().map(|a| a.len()).unwrap_or(0) >= 2);
    fx.post("/api/settings/restore", json!({"version": v1, "path": "prompts/system.md"})).await;
    assert_eq!(std::fs::read_to_string(fx.cfg.join("prompts/system.md")).unwrap_or_default(), "SYS-V1");
    let bad = fx.put("/api/settings/prompts", json!({"path": "../models.toml", "content": "x"})).await;
    assert!(bad["error"].is_string(), "pad buiten prompts/ geweigerd");
}

#[tokio::test]
async fn status_reports_subsystems() {
    let fx = setup().await;
    let s = fx.get("/api/status").await;
    assert_eq!(s["subsystems"][0]["name"], "kern");
    assert_eq!(s["default_model"], "m");
}

async fn delete(fx: &Fx, path: &str) -> Value {
    fx.http
        .delete(format!("{}{path}", fx.base))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"))
        .json()
        .await
        .unwrap_or_default()
}

#[tokio::test]
async fn chat_writes_memory_and_uses_it_next_time() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "Onthoud: mijn favoriete editor is Helix."}))
        .await;
    fx.wait_message(&sid, 1, "complete").await;
    // Herinnering wordt op de achtergrond geschreven.
    let mut found = Value::Null;
    for _ in 0..100 {
        found = fx.get("/api/memory?q=Helix").await;
        if found.as_array().is_some_and(|a| !a.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let arr = found.as_array().cloned().unwrap_or_default();
    assert!(arr.iter().any(|m| m["kind"] == "user"), "persoonlijk feit als user-geheugen: {found}");
    // Volgende vraag: geheugen zit in de system prompt (nep-model echoot hem bij 'welke prompt?').
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "welke prompt?"})).await;
    let c = fx.wait_message(&sid, 3, "complete").await["content"].as_str().unwrap_or_default().to_string();
    assert!(c.contains("Helix") && c.contains("Relevante herinneringen"), "geheugen in prompt: {c}");
    // Retrieval-event gekoppeld aan de run (voor het GUI-paneel).
    let runs = fx.get("/api/runs").await;
    let run = runs["recent"][0]["id"].as_str().unwrap_or_default().to_string();
    let ev = fx.kernel.events().for_run(run.parse().unwrap_or_default()).unwrap_or_default();
    assert!(ev.iter().any(|e| e.kind == vortex_types::EventKind::RetrievalPerformed));
    // Zoeken + vergeten via de API.
    let hits = fx.post("/api/memory/search", json!({"text": "welke editor gebruik ik?"})).await;
    let id = hits[0]["memory"]["id"].as_str().unwrap_or_default().to_string();
    assert!(!id.is_empty(), "{hits}");
    let f = delete(&fx, &format!("/api/memory/{id}")).await;
    assert!(f["forgotten"].as_array().is_some_and(|a| !a.is_empty()));
    let stats = fx.get("/api/memory/stats").await;
    assert!(stats["memories"].as_i64().is_some());
}

#[tokio::test]
async fn eval_feedback_trace_replay_analytics() {
    let fx = setup().await;
    // SAFETY van de test: evals-map uit de repo.
    unsafe_env();
    let suites = fx.get("/api/eval/suites").await;
    assert!(suites.as_array().is_some_and(|a| a.len() >= 5), "{suites}");
    let st = fx.post("/api/eval/run", json!({"suite": "eerlijkheid", "label": "test"})).await;
    assert_eq!(st["started"][0], "eerlijkheid");
    let mut run = Value::Null;
    for _ in 0..200 {
        let rs = fx.get("/api/eval/runs?suite=eerlijkheid").await;
        if rs[0]["finished_at"].is_string() {
            run = rs[0].clone();
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(run["n"], 3, "{run}");
    let d = fx.get(&format!("/api/eval/runs/{}", run["id"].as_str().unwrap_or(""))).await;
    assert_eq!(d["comparison"]["verdict"], "eerste meting");
    assert_eq!(d["results"].as_array().map(|a| a.len()), Some(3));
    // eval-runs liepen als P2 via de Dirigent en lieten geen geheugen achter
    let mem = fx.get("/api/memory?limit=500").await;
    assert!(
        mem.as_array().is_some_and(|a| a.iter().all(|m| !m["workspace"].as_str().unwrap_or("").starts_with("__eval")))
    );
    // retrieval-eval + lekkage
    let r = fx.post("/api/eval/retrieval", json!({})).await;
    assert!(r["recall_at_5"].as_f64().unwrap_or(0.0) > 0.5, "{r}");
    let l = fx.get("/api/eval/leakage").await;
    assert!(l["leaks"].is_array(), "{l}");
    // chat + feedback + trace + replay + analytics
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "hoi daar"})).await;
    let m = fx.wait_message(&sid, 1, "complete").await;
    let fb = fx
        .post(
            &format!("/api/messages/{}/feedback", m["id"].as_str().unwrap_or("")),
            json!({"rating": "correction", "correction": "Mijn naam is Syneye."}),
        )
        .await;
    assert_eq!(fb["ok"], true, "{fb}");
    let tr = fx.get(&format!("/api/runs/{}/trace", m["run_id"].as_str().unwrap_or(""))).await;
    assert!(tr["events"].as_array().is_some_and(|e| e.iter().any(|x| x["kind"] == "context.prompt.assembled")), "{tr}");
    assert!(tr["spans"]["wait_ms"].is_i64());
    let rp = fx.get(&format!("/api/sessions/{sid}/replay")).await;
    assert!(rp["events"].as_array().is_some_and(|e| e.len() > 3));
    let an = fx.get("/api/analytics").await;
    assert!(an["feedback"].as_array().is_some_and(|f| !f.is_empty()), "{an}");
}

fn unsafe_env() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    // Tests draaien in hetzelfde proces; alle tests gebruiken dezelfde waarde.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("VORTEX_EVALS", dir);
    }
}

async fn events_of(fx: &Fx, run: &str) -> Vec<vortex_types::Event> {
    fx.kernel.events().for_run(run.parse().unwrap_or_default()).unwrap_or_default()
}

#[tokio::test]
async fn deep_mode_runs_full_cognitive_cycle_with_debate_team() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let r = fx
        .post(
            &format!("/api/sessions/{sid}/messages"),
            json!({"text": "Vergelijk SQLite en Postgres voor Vortex", "mode": "deep"}),
        )
        .await;
    let run = r["run_id"].as_str().unwrap_or_default().to_string();
    let m = fx.wait_message(&sid, 1, "complete").await;
    assert_eq!(m["content"], "Hallo wereld");
    let ev = events_of(&fx, &run).await;
    let kinds: Vec<_> = ev.iter().map(|e| e.kind).collect();
    use vortex_types::EventKind as K;
    assert_eq!(kinds.iter().filter(|k| **k == K::PhaseCompleted).count(), 8, "alle 8 fasen");
    assert!(kinds.contains(&K::StrategyChosen) && kinds.contains(&K::TeamComposed));
    assert!(kinds.contains(&K::DecisionMade), "router-features gelogd");
    let strat = ev.iter().find(|e| e.kind == K::StrategyChosen).map(|e| e.payload.clone()).unwrap_or_default();
    assert_eq!(strat["path"], "deliberate");
    // Team: rollen gerapporteerd, Judge-gate vóór de Synthesizer.
    let roles: Vec<String> = ev
        .iter()
        .filter(|e| e.kind == K::AgentRoleCompleted)
        .map(|e| e.payload["role"].as_str().unwrap_or("").to_string())
        .collect();
    for want in ["Planner", "Solvers", "Critic", "Fact-checker", "Judge", "Synthesizer"] {
        assert!(roles.iter().any(|r| r == want), "{want} ontbreekt: {roles:?}");
    }
    let judge = ev.iter().position(|e| e.kind == K::JudgeVerdict).unwrap_or(usize::MAX);
    let first_delta = ev.iter().position(|e| e.kind == K::MessageDelta).unwrap_or(0);
    assert!(judge < first_delta, "antwoord pas na het oordeel van de Judge");
    let v = ev.iter().find(|e| e.kind == K::JudgeVerdict).map(|e| e.payload.clone()).unwrap_or_default();
    assert_eq!(v["verdict"]["verdict"], "accept");
    let ev_c = ev.iter().find(|e| e.kind == K::EvidenceCompiled).map(|e| e.payload.clone()).unwrap_or_default();
    assert_eq!(ev_c["scope"], "lokaal", "geen SearXNG → offline, lokaal geverifieerd");
    let ws = fx.get(&format!("/api/runs/{run}/workspace")).await;
    assert_eq!(ws["live"], false);
    assert_eq!(ws["board"]["goals"].as_array().map(|g| g.len()), Some(2), "{ws}");
}

#[tokio::test]
async fn personal_question_without_memory_is_told_not_to_guess() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let r = fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "Wat is de naam van mijn hond?"})).await;
    let run = r["run_id"].as_str().unwrap_or_default().to_string();
    fx.wait_message(&sid, 1, "complete").await;
    let ev = events_of(&fx, &run).await;
    let strat = ev
        .iter()
        .find(|e| e.kind == vortex_types::EventKind::StrategyChosen)
        .map(|e| e.payload.clone())
        .unwrap_or_default();
    assert_eq!(strat["prediction"], "abstain", "{strat}");
    let ver = ev
        .iter()
        .find(|e| e.kind == vortex_types::EventKind::VerificationDone)
        .map(|e| e.payload.clone())
        .unwrap_or_default();
    assert_eq!(ver["ok"], false, "nep-model gokt → verificatie markeert het: {ver}");
}

#[tokio::test]
async fn steering_mid_stream_continues_with_new_instruction() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let r = fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "slow", "mode": "quick"})).await;
    let run = r["run_id"].as_str().unwrap_or_default().to_string();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let st = fx.post(&format!("/api/runs/{run}/steer"), json!({"text": "korter graag"})).await;
    assert_eq!(st["accepted"], true, "{st}");
    let m = fx.wait_message(&sid, 1, "complete").await;
    let c = m["content"].as_str().unwrap_or_default();
    assert!(c.starts_with("w0 "), "begin van het oorspronkelijke antwoord blijft: {c}");
    assert!(c.ends_with("Hallo wereld"), "daarna het bijgestuurde vervolg: {c}");
    assert!(c.len() < 400 * 4);
    assert!(events_of(&fx, &run).await.iter().any(|e| e.kind == vortex_types::EventKind::SteerReceived));
    let ag = fx.get("/api/agenda").await;
    assert!(ag.is_array());
    let pr = fx.get("/api/profile").await;
    assert!(pr["profile"].is_object());
}

#[tokio::test]
async fn chat_uses_tools_with_cards_and_proposes_skill() {
    let fx = setup().await;
    let target = fx._dir.path().join("werk/notitie.txt");
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let r = fx
        .post(
            &format!("/api/sessions/{sid}/messages"),
            json!({"text": format!("TOOLTEST {}", target.display()), "mode": "quick"}),
        )
        .await;
    let run = r["run_id"].as_str().unwrap_or_default().to_string();
    let m = fx.wait_message(&sid, 1, "complete").await;
    assert_eq!(std::fs::read_to_string(&target).unwrap_or_default(), "hallo uit een tool\n");
    let text = m["content"].as_str().unwrap_or("");
    assert!(text.starts_with("Ik schrijf het bestand."), "{text}");
    assert!(text.contains("Klaar, gelezen:") && text.contains("hallo uit een tool"), "{text}");
    // Tool-kaarten: events met run_id, write vóór read (effectanalyse), preview bij write.
    let ev = events_of(&fx, &run).await;
    let done: Vec<&vortex_types::Event> =
        ev.iter().filter(|e| e.kind == vortex_types::EventKind::ToolCallCompleted).collect();
    assert_eq!(done.len(), 2);
    assert_eq!(done[0].payload["tool"], "write_file");
    assert!(done[0].payload["preview"].as_str().unwrap_or("").contains("+hallo uit een tool"));
    // Tools-API.
    let tools = fx.get("/api/tools").await;
    assert!(tools["tools"].as_array().is_some_and(|t| t.iter().any(|x| x["name"] == "memory_search")));
    // Skill-voorstel uit deze geslaagde trace (2 stappen).
    let props = fx.get("/api/skills/proposals").await;
    let p = props.as_array().and_then(|a| a.first()).cloned().unwrap_or_default();
    assert!(p["body"].as_str().unwrap_or("").contains("`write_file`"), "{props}");
    let acc = fx
        .post(&format!("/api/skills/proposals/{}", p["id"].as_str().unwrap_or("")), json!({"status": "accepted"}))
        .await;
    assert_eq!(acc["ok"], true);
    let sk = fx.get("/api/skills").await;
    assert!(sk["skills"].as_array().is_some_and(|a| a.iter().any(|s| s["name"] == acc["name"])));
    // Handmatig aanroepen met preview (dry run): niets toegepast.
    let r = fx
        .post(
            "/api/tools/write_file/call",
            json!({"input": {"path": target.display().to_string(), "content": "anders"}, "dry_run": true}),
        )
        .await;
    assert_eq!(r["ok"], true);
    assert_eq!(std::fs::read_to_string(&target).unwrap_or_default(), "hallo uit een tool\n");
}

#[tokio::test]
async fn ingest_markdown_with_locators_incremental_and_research_api() {
    let fx = setup().await;
    let dir = fx._dir.path().join("docs");
    std::fs::create_dir_all(&dir).unwrap_or_default();
    std::fs::write(dir.join("handleiding.md"), "# Handleiding\nIntro over Vortex.\n## Installatie\nDraai eerst `just models-up` om de llama-servers te starten.\n## Gebruik\nOpen de GUI op poort 7420.\n").unwrap_or_default();
    // Zonder worker → Rust-parser (Markdown).
    let r = fx.post("/api/ingest", json!({"paths": [dir.display().to_string()]})).await;
    assert_eq!(r["done"], true, "{r}");
    assert_eq!(r["report"]["docs"], 1);
    assert_eq!(r["report"]["via_worker"], false);
    let docs = fx.get("/api/ingest/documents").await;
    assert!(
        docs.as_array().is_some_and(|d| d.iter().any(|x| x["uri"].as_str().unwrap_or("").ends_with("handleiding.md"))),
        "{docs}"
    );
    // Doorzoekbaar met exacte bronverwijzing.
    let s = fx.post("/api/memory/search", json!({"text": "llama-servers starten models-up"})).await;
    let hit = s
        .as_array()
        .and_then(|a| a.iter().find(|i| i["memory"]["content"].as_str().unwrap_or("").contains("models-up")))
        .cloned()
        .unwrap_or_default();
    assert_eq!(hit["memory"]["locator"], "regel 3 > Installatie", "{s}");
    // Incrementeel: ongewijzigd = overgeslagen.
    let r2 = fx.post("/api/ingest", json!({"paths": [dir.display().to_string()]})).await;
    assert_eq!(r2["report"]["skipped"], 1);
    // Research-API (quick, offline: alleen lokaal).
    let id =
        fx.post("/api/research", json!({"question": "Hoe start ik de llama-servers?", "mode": "quick"})).await["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
    let mut d = json!({});
    for _ in 0..100 {
        d = fx.get(&format!("/api/research/{id}")).await;
        if d["status"] == "klaar" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(d["status"], "klaar", "{d}");
    assert!(d["report"].as_str().unwrap_or("").contains("## Bronnen (lokaal)"));
    assert!(d["sources"].as_array().is_some_and(|s| s.iter().any(|x| x["locator"] == "regel 3 > Installatie")), "{d}");
    let csv = fx
        .http
        .get(format!("{}/api/research/{id}/export?format=csv", fx.base))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"))
        .text()
        .await
        .unwrap_or_default();
    assert!(csv.starts_with("id,kind,text,status"));
}

/// Gate W10: een ebook (EPUB) via de echte Python-worker → doorzoekbaar met hoofdstuk-locator.
#[tokio::test]
async fn ebook_via_python_worker_is_searchable_with_exact_locator() {
    let worker = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worker");
    if std::process::Command::new("uv").arg("--version").output().is_err() || !worker.join(".venv").exists() {
        return;
    }
    let fx = setup().await;
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        l.local_addr().map(|a| a.port()).unwrap_or(50099)
    };
    std::fs::write(
        fx.cfg.join("paths.toml"),
        format!(
            "worker_address = \"127.0.0.1:{port}\"\ncold_dir = \"{}\"\n",
            fx._dir.path().join("cold").display().to_string().replace('\\', "/")
        ),
    )
    .unwrap_or_default();
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    let book = fx._dir.path().join("boek.epub");
    let mk = format!(
        "import zipfile\nz=zipfile.ZipFile({:?},'w')\nz.writestr('META-INF/container.xml','<?xml version=\"1.0\"?><container xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" version=\"1.0\"><rootfiles><rootfile full-path=\"OEBPS/c.opf\"/></rootfiles></container>')\nz.writestr('OEBPS/c.opf','<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Testboek</dc:title></metadata><manifest><item id=\"a\" href=\"a.xhtml\"/><item id=\"b\" href=\"b.xhtml\"/></manifest><spine><itemref idref=\"a\"/><itemref idref=\"b\"/></spine></package>')\nz.writestr('OEBPS/a.xhtml','<html><body><h1>Begin</h1><p>Het begin van het boek over sterren.</p></body></html>')\nz.writestr('OEBPS/b.xhtml','<html><body><h1>Planeten</h1><h2>Jupiter</h2><p>Jupiter heeft de Grote Rode Vlek, een storm groter dan de aarde.</p></body></html>')\nz.close()\n",
        book.display().to_string()
    );
    let ok = std::process::Command::new("uv")
        .args(["run", "python", "-c", &mk])
        .current_dir(&worker)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "epub maken");
    // Direct de venv-binary (geen `uv run`-tussenproces dat een wees achterlaat).
    let bin = if cfg!(windows) {
        worker.join(".venv/Scripts/vortex-worker.exe")
    } else {
        worker.join(".venv/bin/vortex-worker")
    };
    let mut child = tokio::process::Command::new(bin)
        .args([
            "--address",
            &format!("127.0.0.1:{port}"),
            "--out-dir",
            &fx._dir.path().join("cold/ingest").display().to_string(),
        ])
        .current_dir(&worker)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    // Wachten tot de worker luistert.
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let r = fx.post("/api/ingest", json!({"paths": [book.display().to_string()]})).await;
    let rep = if r["done"] == true { r["report"].clone() } else { json!({}) };
    assert_eq!(rep["via_worker"], true, "{r}");
    assert_eq!(rep["docs"], 1);
    let s = fx.post("/api/memory/search", json!({"text": "Grote Rode Vlek storm Jupiter"})).await;
    let hit = s
        .as_array()
        .and_then(|a| a.iter().find(|i| i["memory"]["content"].as_str().unwrap_or("").contains("Rode Vlek")))
        .cloned()
        .unwrap_or_default();
    assert_eq!(hit["memory"]["locator"], "hoofdstuk 2 > Jupiter", "{s}");
    assert!(hit["memory"]["source_uri"].as_str().unwrap_or("").ends_with("boek.epub"));
    // Ruwe/tussendata op de koude schijf.
    assert!(std::fs::read_dir(fx._dir.path().join("cold/ingest")).map(|d| d.count()).unwrap_or(0) >= 1);
    let _ = child.kill().await;
}
