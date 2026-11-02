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

static LORA_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Zag het nep-model een bericht met afbeeldingen (OpenAI-contentdelen)?
static SAW_IMAGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Zag het nep-model de OCR-onzekerheid in de prompt?
static SAW_UNSURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Nep-llama-server: streamt "Hallo wereld" in stukjes; "slow" streamt lang.
async fn fake_model() -> String {
    async fn chat(Json(b): Json<Value>) -> Response {
        if b["messages"].as_array().is_some_and(|ms| {
            ms.iter().any(|m| m["content"].as_array().is_some_and(|p| p.iter().any(|x| x["type"] == "image_url")))
        }) {
            SAW_IMAGE.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if b["messages"].to_string().contains("ONZEKER gelezen") {
            SAW_UNSURE.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let last = b["messages"]
            .as_array()
            .and_then(|m| m.last())
            .and_then(|m| m["content"].as_str())
            .unwrap_or("")
            .to_string();
        if b["logprobs"].as_bool().unwrap_or(false) {
            let lp = json!({"content": [
                {"token": "De", "logprob": -0.01, "top_logprobs": [{"token": "De", "logprob": -0.01}]},
                {"token": " hoofdstad", "logprob": -0.05, "top_logprobs": []},
                {"token": " is", "logprob": -0.02, "top_logprobs": []},
                {"token": " Lyon", "logprob": -1.8, "top_logprobs": [{"token": " Lyon", "logprob": -1.8}, {"token": " Parijs", "logprob": -0.3}]}
            ]});
            return axum::Json(json!({"choices":[{"message":{"content": "De hoofdstad is Lyon"},"finish_reason":"stop","logprobs": lp}],"usage":{"prompt_tokens":3,"completion_tokens":4}})).into_response();
        }
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
    // LoRA-hot-swap (W13.04): één geladen adapter; POST zet de schaal (onthouden in LORA_SCALE).
    let lora = Router::new().route(
        "/lora-adapters",
        get(|| async { axum::Json(json!([{"id": 0, "path": "/adapters/stijl.gguf", "scale": f32::from_bits(LORA_SCALE.load(std::sync::atomic::Ordering::SeqCst))}])) }).post(
            |Json(b): Json<Value>| async move {
                if let Some(sc) = b[0]["scale"].as_f64() {
                    LORA_SCALE.store((sc as f32).to_bits(), std::sync::atomic::Ordering::SeqCst);
                }
                axum::Json(json!({"success": true}))
            },
        ),
    );
    let app =
        Router::new().route("/health", get(|| async { "ok" })).route("/v1/chat/completions", post(chat)).merge(lora);
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

/// Start de echte Python-worker (venv-binary) op een vrije poort en zet `paths.worker_address`.
async fn start_worker(fx: &Fx) -> Option<tokio::process::Child> {
    let worker = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worker");
    let bin = if cfg!(windows) {
        worker.join(".venv/Scripts/vortex-worker.exe")
    } else {
        worker.join(".venv/bin/vortex-worker")
    };
    if !bin.exists() {
        return None;
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    std::fs::write(
        fx.cfg.join("paths.toml"),
        format!(
            "worker_address = \"127.0.0.1:{port}\"\ncold_dir = \"{}\"\n",
            fx._dir.path().join("cold").display().to_string().replace('\\', "/")
        ),
    )
    .ok()?;
    fx.kernel.config().reload().ok()?;
    let child = tokio::process::Command::new(bin)
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
        .ok()?;
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Some(child)
}

/// Gate W11 end-to-end: voorbeelden + labels → training via de worker → eval → automatische
/// promotie (net wint van heuristiek) → rollback via de API.
#[tokio::test]
async fn neural_layer_trains_promotes_and_rolls_back() {
    let fx = setup().await;
    let Some(mut worker) = start_worker(&fx).await else { return };
    // 160 salience-voorbeelden: waarheid x0 + x2 > 1; heuristiek kijkt alleen naar x0.
    let mut s: u64 = 42;
    for i in 0..160 {
        let mut x = Vec::new();
        for _ in 0..6 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            x.push(((s >> 33) as f64) / (1u64 << 31) as f64);
        }
        let r = format!("run-{i}");
        let e = vortex_types::Event::new(
            vortex_types::EventKind::NeuralExample,
            vortex_types::TraceId::new(),
            json!({"layer": "salience", "x": x, "h": [if x[0] > 0.5 { 0.9 } else { 0.1 }], "ref": r}),
        );
        fx.kernel.events().append(e).unwrap_or_default();
        fx.post(
            "/api/neural/salience/label",
            json!({"reference": r, "label": if x[0] + x[2] > 1.0 { 1.0 } else { 0.0 }}),
        )
        .await;
    }
    let st = fx.get("/api/neural").await;
    assert_eq!(st["examples"]["salience"], 160, "{st}");
    let r = fx.post("/api/neural/salience/train", json!({})).await;
    assert!(r["decision"].as_str().unwrap_or("").contains("gepromoveerd"), "{r}");
    assert!(r["eval"]["net"].as_f64().unwrap_or(0.0) > r["eval"]["heuristic"].as_f64().unwrap_or(1.0), "{r}");
    let st = fx.get("/api/neural").await;
    let sal =
        st["layers"].as_array().and_then(|a| a.iter().find(|l| l["layer"] == "salience")).cloned().unwrap_or_default();
    assert_eq!(sal["manifest"]["mode"], "active");
    assert!(st["runs"].as_array().is_some_and(|a| !a.is_empty()));
    let rb = fx.post("/api/neural/salience/rollback", json!({})).await;
    assert!(rb["active"].is_null(), "{rb}");
    let _ = worker.kill().await;
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

#[tokio::test]
async fn advanced_memory_endpoints() {
    let fx = setup().await;
    for t in ["Syneye woont in Utrecht.", "Utrecht ligt in Nederland.", "Nederland ligt in Europa."] {
        fx.post("/api/memory", json!({"content": t, "kind": "semantic"})).await;
    }
    let r = fx.post("/api/memory/reason", json!({})).await;
    assert!(r["derived"].as_u64().unwrap_or(0) >= 2, "{r}");
    let g = fx.get("/api/memory/graph?entity=utrecht").await;
    assert!(g.as_array().is_some_and(|a| a.iter().any(|f| f["derived"] == true)), "{g}");
    let d = fx.post("/api/memory/dream", json!({})).await;
    assert!(d["dream"]["ms"].is_number(), "{d}");
    assert_eq!(fx.get("/api/memory/dreams").await.as_array().map(|a| a.len()), Some(1));
    let m = fx.get("/api/memory/meta").await;
    assert!(m["total"].as_i64().unwrap_or(0) >= 3, "{m}");
    let b = fx.get("/api/memory/belief-experiment?n=100").await;
    assert_eq!(b["scenarios"].as_array().map(|a| a.len()), Some(3), "{b}");
    let t = fx.get("/api/memory/tier").await;
    assert!(t["hot"].as_i64().unwrap_or(0) >= 3);
    let img = fx._dir.path().join("foto.png");
    std::fs::write(&img, [0x89u8, b'P', b'N', b'G']).unwrap_or_default();
    let w = fx
        .post(
            "/api/memory/media",
            json!({"path": img.display().to_string(), "description": "Foto van de nieuwe werkplek met twee monitoren"}),
        )
        .await;
    let id = w["ids"][0].as_str().unwrap_or_default().to_string();
    let bytes =
        fx.http.get(format!("{}/api/memory/{id}/media", fx.base)).send().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(bytes.headers().get("content-type").and_then(|v| v.to_str().ok()), Some("image/png"));
    let sm = fx.get("/api/selfmodel").await;
    assert!(sm["by_task"].is_object(), "{sm}");
}

#[tokio::test]
async fn training_datasets_hot_swap_gate_and_worker_job() {
    let fx = setup().await;
    // Team-run in deep mode → kandidaten vastgelegd; daarna 👍 → SFT-voorbeeld.
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(
        &format!("/api/sessions/{sid}/messages"),
        json!({"text": "Vergelijk SQLite en Postgres voor Vortex", "mode": "deep"}),
    )
    .await;
    let m = fx.wait_message(&sid, 1, "complete").await;
    fx.post(&format!("/api/messages/{}/feedback", m["id"].as_str().unwrap_or("")), json!({"rating": "good"})).await;
    // Extra DPO-paar met duidelijk scoreverschil.
    let e = vortex_types::Event::new(
        vortex_types::EventKind::CandidatesRecorded,
        vortex_types::TraceId::new(),
        json!({"question": "Wat is 2+2?", "verdict": {"verdict": "accept", "confidence": 0.9}, "candidates": [
            {"id": "k1", "text": "4", "score": 0.9, "chosen": true, "objective": true},
            {"id": "k2", "text": "5", "score": 0.1, "chosen": false, "objective": false}]}),
    );
    fx.kernel.events().append(e).unwrap_or_default();
    let sft = fx.post("/api/training/datasets/sft", json!({})).await;
    assert!(sft["n"].as_i64().unwrap_or(0) >= 1, "{sft}");
    let dpo = fx.post("/api/training/datasets/dpo", json!({})).await;
    assert!(dpo["n"].as_i64().unwrap_or(0) >= 1, "{dpo}");
    // Adapter registreren (zoals na training) → hot-swap aan/uit via /lora-adapters.
    let now = vortex_types::now().to_rfc3339();
    fx.kernel
        .events()
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO adapters(id, base_model, kind, dataset, settings, gguf_path, status, created_at, updated_at) VALUES ('ad_test', 'base', 'sft', ?1, '{}', '/adapters/stijl.gguf', 'trained', ?2, ?2)",
                rusqlite::params![sft["version"].as_str().unwrap_or(""), now],
            )?;
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let r = fx.post("/api/training/adapters/ad_test/scale", json!({"scale": 1.0})).await;
    assert_eq!(r["status"], "active", "{r}");
    assert_eq!(f32::from_bits(LORA_SCALE.load(std::sync::atomic::Ordering::SeqCst)), 1.0);
    // Poortwachter zonder basis-eval: geen winst aantoonbaar → afgewezen, adapter weer uit.
    let g = fx.post("/api/training/adapters/ad_test/gate", json!({})).await;
    assert_eq!(g["status"], "rejected", "{g}");
    assert_eq!(f32::from_bits(LORA_SCALE.load(std::sync::atomic::Ordering::SeqCst)), 0.0);
    // Kalibratie vraagt genoeg gelabelde oordelen.
    let c = fx.post("/api/training/calibrate", json!({})).await;
    assert!(c["error"].as_str().unwrap_or("").contains("te weinig"), "{c}");
    let st = fx.get("/api/training").await;
    assert!(st["datasets"].as_array().is_some_and(|d| d.len() >= 2));
    // Training via de echte worker: valideert de dataset; zonder PyTorch in de cloud → nette fout.
    if let Some(mut w) = start_worker(&fx).await {
        let t = fx.post("/api/training/train", json!({"kind": "sft", "dataset": sft["version"]})).await;
        let err = t["error"].as_str().unwrap_or("");
        assert!(err.contains("PyTorch") || err.contains("torch") || t["gguf"].is_string(), "{t}");
        let st = fx.get("/api/training").await;
        assert!(
            st["adapters"]
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x["status"] == "failed" || x["status"] == "trained")),
            "{st}"
        );
        let _ = w.kill().await;
    }
}

/// W14: wereldmonitor via de API (nepbronnen), gevolgde site, voorspellingen, spelers en een
/// HF-dataset (lokaal bestand) via de echte worker naar kennis, SFT en layer + catalogus.
#[tokio::test]
async fn world_monitor_api_and_hf_import() {
    let fx = setup().await;
    let now = chrono::Utc::now().to_rfc2822();
    let n2 = now.clone();
    let app = axum::Router::new()
        .route("/a", axum::routing::get(move || { let d = now.clone(); async move { format!("<rss><channel><item><title>Strong earthquake strikes coast of Japan</title><link>https://a.example/1</link><pubDate>{d}</pubDate></item><item><title>Dutch cabinet presents budget plan</title><link>https://a.example/2</link><pubDate>{d}</pubDate></item></channel></rss>") } }))
        .route("/b", axum::routing::get(move || { let d = n2.clone(); async move { format!("<rss><channel><item><title>Earthquake strikes Japan coast, tsunami warning</title><link>https://b.example/1</link><pubDate>{d}</pubDate></item></channel></rss>") } }))
        .route("/page", axum::routing::get(|| async { axum::response::Html("<html><title>Blog</title><main><p>Nieuwe post over modellen en geheugen, lang genoeg om als tekst te tellen voor de extractor van Vortex.</p></main></html>") }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("{e}"));
    let web = format!("http://{}", l.local_addr().unwrap_or_else(|e| panic!("{e}")));
    tokio::spawn(async move { axum::serve(l, app).await });
    std::fs::write(
        fx.cfg.join("world.toml"),
        format!("enabled = false\n[[sources]]\nname = \"a\"\nkind = \"rss\"\nurl = \"{web}/a\"\ncountry = \"NL\"\n[[sources]]\nname = \"b\"\nkind = \"rss\"\nurl = \"{web}/b\"\ncountry = \"UK\"\n"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    let f = fx.post("/api/world/follow", json!({"url": format!("{web}/page"), "interval_hours": 1})).await;
    assert_eq!(f["followed"].as_array().map(Vec::len), Some(1), "{f}");
    let r = fx.post("/api/world/fetch", json!({})).await;
    assert_eq!(r["fetched"].as_array().map(Vec::len), Some(2), "{r}");
    assert_eq!(r["cluster"]["confirmed"], 1, "{r}");
    assert_eq!(r["followed_changed"].as_array().map(Vec::len), Some(1), "{r}");
    let st = fx.get("/api/world").await;
    assert!(st["sources"][0]["terms"].as_str().unwrap_or("").contains("RSS"), "{st}");
    let ev = fx.get("/api/world/events?status=confirmed").await;
    let id = ev[0]["id"].as_str().unwrap_or_default().to_string();
    assert_eq!(fx.get(&format!("/api/world/events/{id}")).await["items"].as_array().map(Vec::len), Some(2));
    assert!(fx.get("/api/world/bias").await.as_array().is_some_and(|b| b.len() >= 2));
    assert!(fx.get("/api/world/items?q=earthquake").await.as_array().is_some_and(|v| v.len() == 2));
    // Voorspelling met verlopen deadline → toetsen → niet meer open.
    let p = fx.post("/api/world/predictions", json!({"claim": "Japan meldt een naschok", "probability": 0.7, "criteria": "earthquake Japan", "resolve_by": "2000-01-01"})).await;
    assert_eq!(p["status"], "open", "{p}");
    let res = fx.post("/api/world/predictions/resolve", json!({})).await;
    assert_ne!(res[0]["status"], "open", "{res}");
    assert!(fx.get("/api/world/calibration").await["counts"].is_object());
    // Spelers: privépersoon zonder publieke rol geweigerd.
    let bad = fx
        .http
        .post(format!("{}/api/world/players", fx.base))
        .json(&json!({"name": "Piet", "sector": "x", "kind": "person"}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(bad.status(), 400);
    let pl = fx.post("/api/world/players", json!({"name": "Acme", "sector": "chips", "kind": "organization"})).await;
    assert!(pl["id"].as_str().is_some());

    // HF-dataset via de worker.
    let Some(mut worker) = start_worker(&fx).await else { return };
    let src = fx._dir.path().join("qa.jsonl");
    let rows: String = (0..12).map(|i| format!("{}\n", json!({"question": format!("Wat is de uitkomst van {i} keer drie, stap voor stap?"), "answer": format!("Het antwoord is {}", i * 3), "label": "math"}))).collect();
    std::fs::write(&src, rows).unwrap_or_else(|e| panic!("{e}"));
    let info =
        fx.post("/api/world/datasets/inspect", json!({"source": src.display().to_string(), "license": "cc0"})).await;
    assert_eq!(info["schema"]["kind"], "qa", "{info}");
    let mapping = info["mapping"].clone();
    for dest in ["knowledge", "sft"] {
        let r = fx
            .post(
                "/api/world/datasets/import",
                json!({"source": src.display().to_string(), "destination": dest, "mapping": mapping, "license": "cc0"}),
            )
            .await;
        assert_eq!(r["stats"]["kept"], 12, "{dest}: {r}");
    }
    let lay = fx.post("/api/world/datasets/import", json!({"source": src.display().to_string(), "destination": "layer", "kind": "text", "mapping": {"text": "question", "label": "label"}})).await;
    assert_eq!(lay["stats"]["kept"], 12, "{lay}");
    let none = fx
        .post(
            "/api/world/datasets/import",
            json!({"source": src.display().to_string(), "destination": "sft", "mapping": {}}),
        )
        .await;
    assert!(none["stats"].is_null(), "zonder bevestigde mapping geen import");
    let cat = fx.get("/api/world/datasets").await;
    assert_eq!(cat.as_array().map(Vec::len), Some(3), "{cat}");
    assert!(cat.as_array().is_some_and(|c| c.iter().any(|x| x["license"] == "cc0")));
    let _ = worker.kill().await;
}

async fn wait_task(fx: &Fx, id: &str) -> Value {
    for _ in 0..200 {
        let t = fx.get(&format!("/api/autonomy/tasks/{id}")).await;
        if ["done", "failed", "cancelled"].contains(&t["status"].as_str().unwrap_or("")) {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("taak {id} niet klaar");
}

/// W15: taakqueue + executor (afhankelijkheden, project), triggers, suggesties → taak, briefing,
/// backup + restore-test, voortgang, OpenAI-compatibele API (stream en niet-stream).
#[tokio::test]
async fn autonomy_tasks_triggers_briefing_backup_openai() {
    let fx = setup().await;
    std::fs::write(fx.cfg.join("autonomy.toml"), "enabled = true\n").unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    // Project met twee taken; de tweede wacht op de eerste.
    let p = fx.post("/api/autonomy/projects", json!({"name": "Onderzoek", "goal": "test"})).await;
    let pid = p["id"].as_str().unwrap_or_default().to_string();
    let a = fx.post("/api/autonomy/tasks", json!({"title": "Vraag 1", "kind": "prompt", "params": {"prompt": "Wat is 2+2?"}, "priority": 1, "needs_idle": false, "project_id": pid})).await;
    let b = fx.post("/api/autonomy/tasks", json!({"title": "Vraag 2", "kind": "prompt", "params": {"prompt": "En 3+3?"}, "priority": 1, "needs_idle": false, "project_id": pid, "depends_on": [a["id"]]})).await;
    let tb = wait_task(&fx, b["id"].as_str().unwrap_or_default()).await;
    assert_eq!(tb["status"], "done", "{tb}");
    let ta = fx.get(&format!("/api/autonomy/tasks/{}", a["id"].as_str().unwrap_or_default())).await;
    assert!(ta["updated_at"].as_str() <= tb["updated_at"].as_str(), "volgorde via afhankelijkheid");
    let proj = fx.get(&format!("/api/autonomy/projects/{pid}")).await;
    assert_eq!(proj["progress"], 1.0, "{proj}");
    assert!(proj["notes"].as_str().unwrap_or("").contains("Vraag 2 klaar"), "{proj}");
    // Onbekende soort → mislukt na de pogingen (max 1).
    let bad = fx
        .post(
            "/api/autonomy/tasks",
            json!({"title": "x", "kind": "bestaat-niet", "priority": 1, "needs_idle": false, "max_attempts": 1}),
        )
        .await;
    assert_eq!(wait_task(&fx, bad["id"].as_str().unwrap_or_default()).await["status"], "failed");
    // Standaardtriggers aanwezig; event-trigger vuurt bij een briefing.
    let trg = fx.get("/api/autonomy/triggers").await;
    assert!(
        trg.as_array().is_some_and(|t| t.iter().any(|x| x["name"] == "briefing" && x["next"].is_string())),
        "{trg}"
    );
    fx.post("/api/autonomy/triggers", json!({"name": "na briefing", "kind": "event", "spec": "autonomy.briefing.written", "task": {"title": "Monitoring na briefing", "kind": "monitor", "priority": 2, "needs_idle": false}})).await;
    // Suggesties: nog geen backup → suggestie met actie → accepteren = taak.
    fx.post("/api/autonomy/suggestions/scan", json!({})).await;
    let list = fx.get("/api/autonomy/suggestions?status=new").await;
    let sug = list
        .as_array()
        .and_then(|v| v.iter().find(|s| s["kind"] == "backup").cloned())
        .unwrap_or_else(|| panic!("{list}"));
    let acc = fx
        .post(&format!("/api/autonomy/suggestions/{}/accept", sug["id"].as_str().unwrap_or_default()), json!({}))
        .await;
    let bt = wait_task(&fx, acc["task"]["id"].as_str().unwrap_or_default()).await;
    assert_eq!(bt["status"], "done", "{bt}");
    assert_eq!(bt["result"]["verified"], true, "restore-test: {bt}");
    let bks = fx.get("/api/autonomy/backups").await;
    assert_eq!(bks["backups"][0]["verify"]["integrity"], "ok", "{bks}");
    assert!(std::path::Path::new(bks["backups"][0]["path"].as_str().unwrap_or("")).exists());
    // Briefing (met model) + event-trigger.
    let br = fx.post("/api/autonomy/briefings", json!({})).await;
    assert!(br["markdown"].as_str().unwrap_or("").starts_with("# Briefing"), "{br}");
    let mut fired = false;
    for _ in 0..100 {
        let ts = fx.get("/api/autonomy/tasks").await;
        if ts.as_array().is_some_and(|v| v.iter().any(|t| t["title"] == "Monitoring na briefing")) {
            fired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(fired, "event-trigger vuurde niet");
    assert!(fx.get("/api/autonomy/progress").await["eval_by_suite"].is_object());
    assert!(fx.get("/api/autonomy").await["queued"].is_number());

    // OpenAI-compatibele API.
    let models = fx.get("/v1/models").await;
    assert!(models["data"].as_array().is_some_and(|d| d.iter().any(|m| m["id"] == "vortex")));
    let body = json!({"model": "vortex-quick", "messages": [{"role": "system", "content": "client"}, {"role": "user", "content": "Hallo API"}]});
    let r = fx.post("/v1/chat/completions", body.clone()).await;
    assert_eq!(r["object"], "chat.completion", "{r}");
    assert!(!r["choices"][0]["message"]["content"].as_str().unwrap_or("").is_empty(), "{r}");
    let mut sb = body.clone();
    sb["stream"] = json!(true);
    sb["messages"] = json!([{"role": "user", "content": "Hallo API"}, {"role": "assistant", "content": "…"}, {"role": "user", "content": "nog een"}]);
    let sse = fx
        .http
        .post(format!("{}/v1/chat/completions", fx.base))
        .json(&sb)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"))
        .text()
        .await
        .unwrap_or_default();
    assert!(sse.contains("chat.completion.chunk") && sse.trim_end().ends_with("data: [DONE]"), "{sse}");
    let n: i64 = fx
        .kernel
        .events()
        .db()
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM api_sessions", [], |r| r.get(0))?))
        .unwrap_or(0);
    assert_eq!(n, 1, "zelfde gesprek → zelfde Vortex-sessie");
    // Doorgeven aan een gewoon model.
    let pt =
        fx.post("/v1/chat/completions", json!({"model": "m", "messages": [{"role": "user", "content": "hoi"}]})).await;
    assert!(pt["choices"].is_array(), "{pt}");
}

/// W15.01: beheerde llama-server (test-dubbel) wordt bij externe GPU-last ontladen en daarna
/// weer geladen; een chatverzoek laadt meteen.
#[cfg(unix)]
#[tokio::test]
async fn gpu_guard_releases_and_restores() {
    let fx = setup().await;
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vortex-models/tests/fake_llama_server.py");
    let model = fx._dir.path().join("lm.gguf");
    std::fs::write(&model, "model").unwrap_or_else(|e| panic!("{e}"));
    let port =
        std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(18999);
    let models = std::fs::read_to_string(fx.cfg.join("models.toml")).unwrap_or_default();
    std::fs::write(
        fx.cfg.join("models.toml"),
        format!("{models}[servers.gpu]\nurl = \"http://127.0.0.1:{port}\"\nmodel = \"lm\"\nmanaged = true\nlaunch_args = [\"{}\"]\n[models.lm]\nfile = \"{}\"\nroles = [\"chat\"]\ngpu = \"gpu\"\n", script.display(), model.display()),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(fx.cfg.join("paths.toml"), "llama_server = \"python3\"\n").unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(fx.cfg.join("autonomy.toml"), "[gpu]\nrestore_after_secs = 0\n").unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    let g = vortex_server::autonomy::gpu::GpuGuard::new(fx.kernel.events().clone());
    g.configure(&fx.kernel, fx._dir.path());
    assert_eq!(g.managed(), vec!["gpu".to_string()]);
    let r = g.load_all().await;
    assert_eq!(r[0]["ok"], true, "{r:?}");
    assert_eq!(g.step(true).await, Some("released"), "game → GPU vrij");
    assert!(g.is_released());
    assert!(reqwest::get(format!("http://127.0.0.1:{port}/health")).await.is_err(), "server echt gestopt");
    assert_eq!(g.step(true).await, None, "al vrij");
    assert_eq!(g.step(false).await, Some("restored"));
    assert_eq!(
        reqwest::get(format!("http://127.0.0.1:{port}/health")).await.map(|r| r.status().as_u16()).unwrap_or(0),
        200
    );
    // Expliciet verzoek tijdens "gamen": ensure laadt meteen.
    g.step(true).await;
    let reg = vortex_models::Registry::from_config(&fx.kernel.config().current()).unwrap_or_else(|e| panic!("{e}"));
    g.ensure(&reg, "lm").await;
    assert_eq!(
        reqwest::get(format!("http://127.0.0.1:{port}/health")).await.map(|r| r.status().as_u16()).unwrap_or(0),
        200
    );
    let kinds: Vec<String> = fx
        .kernel
        .events()
        .read_from(0, 10_000)
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.kind.as_str().to_string())
        .collect();
    assert!(kinds.iter().any(|k| k == "autonomy.gpu.released") && kinds.iter().any(|k| k == "autonomy.gpu.restored"));
    let all = g.status().await;
    assert_eq!(all["servers"][0]["phase"], "Ready", "{all}");
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn wav(secs: f32) -> Vec<u8> {
    let rate = 16000u32;
    let n = (rate as f32 * secs) as u32;
    let mut v = Vec::new();
    v.extend(b"RIFF");
    v.extend((36 + n * 2).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(16u32.to_le_bytes());
    v.extend(1u16.to_le_bytes());
    v.extend(1u16.to_le_bytes());
    v.extend(rate.to_le_bytes());
    v.extend((rate * 2).to_le_bytes());
    v.extend(2u16.to_le_bytes());
    v.extend(16u16.to_le_bytes());
    v.extend(b"data");
    v.extend((n * 2).to_le_bytes());
    for i in 0..n {
        v.extend(((((i as f32) * 0.05).sin() * 3000.0) as i16).to_le_bytes());
    }
    v
}

/// W16 end-to-end: beeld (visie met afbeeldingen naar het model + OCR met onzekerheid) als
/// chatbijlage, audio (transcript met tijdstempels), video (tijdlijn), spreadsheet (cel-locators),
/// spraak uit, beeldgeneratie — alles in het geheugen en terug te vinden.
#[tokio::test]
async fn multimodal_attachments_memory_and_outputs() {
    let fx = setup().await;
    let Some(mut worker) = start_worker(&fx).await else { return };
    std::fs::write(
        fx.cfg.join("media.toml"),
        "ocr_engine = \"test\"\nstt_engine = \"test\"\ntts_engine = \"test\"\nimagegen_engine = \"test\"\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let models = std::fs::read_to_string(fx.cfg.join("models.toml")).unwrap_or_default();
    std::fs::write(
        fx.cfg.join("models.toml"),
        format!("{models}[models.v]\nfile = \"v.gguf\"\nroles = [\"vision\"]\ngpu = \"main\"\n"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(fx.get("/api/media").await["vision"], true);

    // Beeld als chatbijlage.
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1600, 900)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap_or_else(|e| panic!("{e}"));
    let up = fx.post("/api/uploads", json!({"name": "factuur scan.png", "data": b64(png.get_ref())})).await;
    let path = up["path"].as_str().unwrap_or_default().to_string();
    assert_eq!(up["kind"], "image", "{up}");
    std::fs::write(
        format!("{path}.ocr.json"),
        json!([{"text": "Factuur Zonnepanelen 2026", "conf": 0.95}, {"text": "Totaal 1?84,00", "conf": 0.35}])
            .to_string(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(
        &format!("/api/sessions/{sid}/messages"),
        json!({"text": "Wat staat er op deze factuur?", "attachments": [path]}),
    )
    .await;
    fx.wait_message(&sid, 1, "complete").await;
    assert!(SAW_IMAGE.load(std::sync::atomic::Ordering::SeqCst), "visiemodel kreeg de afbeelding");
    let evs: Vec<Value> = fx
        .kernel
        .events()
        .read_from(0, 100_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind.as_str() == "chat.attachments.processed")
        .map(|e| e.payload)
        .collect();
    assert_eq!(evs[0]["items"][0]["ok"], true, "{evs:?}");
    assert_eq!(evs[0]["items"][0]["result"]["ocr"]["uncertain"].as_array().map(Vec::len), Some(1));
    // Het model kreeg de onzekere regel mee.
    assert!(SAW_UNSURE.load(std::sync::atomic::Ordering::SeqCst), "onzekerheid doorgegeven aan het model");
    let hits = fx.post("/api/memory/search", json!({"text": "factuur zonnepanelen"})).await;
    let hit = hits
        .as_array()
        .and_then(|a| {
            a.iter().find(|h| h["memory"]["content"].as_str().unwrap_or("").contains("Zonnepanelen")).cloned()
        })
        .unwrap_or_else(|| panic!("{hits}"));
    assert_eq!(hit["memory"]["source_uri"].as_str(), Some(path.as_str()));

    // Audio → transcript met tijdstempels in het geheugen.
    let au = fx.post("/api/uploads", json!({"name": "memo.wav", "data": b64(&wav(1.0))})).await;
    let ap = au["path"].as_str().unwrap_or_default().to_string();
    std::fs::write(
        format!("{ap}.stt.json"),
        json!([{"start": 1.0, "end": 4.0, "text": "Morgen de tandarts om negen uur", "conf": 0.9}]).to_string(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let a = fx.post("/api/media/analyze", json!({"path": ap})).await;
    assert!(a["text"].as_str().unwrap_or("").contains("[00:01] Morgen de tandarts"), "{a}");
    let hits = fx.post("/api/memory/search", json!({"text": "tandarts morgen"})).await;
    assert!(hits.as_array().is_some_and(|h| h.iter().any(|x| x["memory"]["locator"] == "00:01–00:04")), "{hits}");

    // Spreadsheet → tabel met cel-locator.
    let csv = fx
        .post(
            "/api/uploads",
            json!({"name": "budget.csv", "data": b64(b"post,bedrag\nhuur,950\nenergie,140\nverzekering,85\n")}),
        )
        .await;
    let r = fx.post("/api/media/analyze", json!({"path": csv["path"]})).await;
    assert_eq!(r["result"]["kind"], "document", "{r}");
    let hits = fx.post("/api/memory/search", json!({"text": "energie bedrag budget"})).await;
    assert!(
        hits.as_array()
            .is_some_and(|h| h.iter().any(|x| x["memory"]["locator"].as_str().unwrap_or("").contains("!A1:B4"))),
        "{hits}"
    );

    // Video (als ffmpeg er is): tijdlijn met beeld + spraak.
    if std::process::Command::new("ffmpeg").arg("-version").output().is_ok() {
        let v = fx._dir.path().join("clip.mp4");
        let ok = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=160x120:d=3",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=160x120:d=3",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=300:duration=6",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0[v]",
                "-map",
                "[v]",
                "-map",
                "2:a",
                "-shortest",
            ])
            .arg(&v)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            std::fs::write(format!("{}.stt.json", v.display()), json!([{"start": 0.2, "end": 2.0, "text": "rood scherm", "conf": 0.9}, {"start": 3.5, "end": 5.5, "text": "blauw scherm", "conf": 0.9}]).to_string()).unwrap_or_else(|e| panic!("{e}"));
            let r = fx.post("/api/media/analyze", json!({"path": v.display().to_string()})).await;
            let t = r["text"].as_str().unwrap_or("");
            assert!(t.contains("spraak: rood scherm") && t.contains("spraak: blauw scherm"), "{r}");
        }
    }

    // Uitvoer: spraak en beeld.
    let sp = fx.post("/api/media/speak", json!({"text": "Hallo. Dit is Vortex."})).await;
    assert_eq!(sp["parts"].as_array().map(Vec::len), Some(2), "{sp}");
    let audio = fx
        .http
        .get(format!("{}{}", fx.base, sp["parts"][0]["url"].as_str().unwrap_or("")))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(audio.headers().get("content-type").and_then(|v| v.to_str().ok()), Some("audio/wav"));
    let im = fx.post("/api/media/imagine", json!({"prompt": "een vuurtoren bij nacht"})).await;
    let img = fx
        .http
        .get(format!("{}{}", fx.base, im["url"].as_str().unwrap_or("")))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(img.status(), 200, "{im}");
    assert!(img.bytes().await.map(|b| b.starts_with(b"\x89PNG")).unwrap_or(false));
    // `/beeld` in de chat → antwoord met het beeld.
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "/beeld een kat op de maan"})).await;
    let m = fx.wait_message(&sid, 3, "complete").await;
    assert!(m["content"].as_str().unwrap_or("").starts_with("![een kat op de maan](/api/media/file/img_"), "{m}");
    let _ = worker.kill().await;
}

/// W17.11: tweede keer dezelfde (informatieve) vraag → uit de cache, model niet opnieuw;
/// persoonlijke vraag nooit uit de cache; slechte feedback → weg uit de cache.
#[tokio::test]
async fn semantic_cache_strict_and_invalidated() {
    let fx = setup().await;
    let ask = |sid: String, q: &'static str| {
        let fx = &fx;
        async move {
            fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": q, "mode": "quick"})).await;
        }
    };
    let s1 = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    ask(s1.clone(), "Wat is de hoofdstad van Frankrijk?").await;
    let first = fx.wait_message(&s1, 1, "complete").await;
    let s2 = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    ask(s2.clone(), "wat is de hoofdstad van frankrijk").await;
    let second = fx.wait_message(&s2, 1, "complete").await;
    assert_eq!(first["content"], second["content"]);
    let hits = || {
        fx.kernel
            .events()
            .read_from(0, 100_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.kind.as_str() == "chat.cache.hit")
            .count()
    };
    assert_eq!(hits(), 1, "tweede keer uit de cache");
    // Andere vraag → geen treffer.
    ask(s2.clone(), "Wat is de hoofdstad van Duitsland?").await;
    fx.wait_message(&s2, 3, "complete").await;
    assert_eq!(hits(), 1);
    // Slechte feedback → weg.
    fx.post(&format!("/api/messages/{}/feedback", first["id"].as_str().unwrap_or("")), json!({"rating": "bad"})).await;
    let s3 = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    ask(s3.clone(), "Wat is de hoofdstad van Frankrijk?").await;
    fx.wait_message(&s3, 1, "complete").await;
    assert_eq!(hits(), 1, "na slechte feedback niet meer uit de cache");
}

async fn wait_exp(fx: &Fx, id: &str) -> Value {
    for _ in 0..600 {
        let e = fx.get(&format!("/api/experiments/{id}")).await;
        if e["status"] != "running" {
            return e;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("experiment {id} niet klaar");
}

/// W17.03/04/05/07: A/B met gepaarde statistiek, schaduw-run met blinde Judge, metamorf +
/// counterfactual, positioneel per model; ablation (🔒) alleen met bevestiging.
#[tokio::test]
async fn experiments_ab_shadow_metamorphic_positional() {
    unsafe_env();
    let fx = setup().await;
    let ab = fx
        .post("/api/experiments/ab", json!({"suites": ["redeneren"], "a": {"name": "basis"}, "b": {"name": "korte prompt", "system_prompt": "Antwoord kort."}}))
        .await;
    let r = wait_exp(&fx, ab["id"].as_str().unwrap_or_default()).await;
    assert_eq!(r["status"], "done", "{r}");
    assert!(r["result"]["overall"]["n"].as_u64().unwrap_or(0) >= 5, "{r}");
    assert!(r["result"]["overall"]["verdict"].is_string());
    // Schaduw: eerst een echte vraag.
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    fx.post(
        &format!("/api/sessions/{sid}/messages"),
        json!({"text": "Wat is het verschil tussen TCP en UDP?", "mode": "quick"}),
    )
    .await;
    fx.wait_message(&sid, 1, "complete").await;
    let sh = fx.post("/api/experiments/shadow", json!({"candidate": {"name": "t0", "temperature": 0.0}, "n": 3})).await;
    let r = wait_exp(&fx, sh["id"].as_str().unwrap_or_default()).await;
    assert_eq!(r["result"]["n"], 1, "{r}");
    assert!(r["result"]["advice"].is_string());
    let mm = fx.post("/api/experiments/metamorphic", json!({"suites": ["redeneren"]})).await;
    let r = wait_exp(&fx, mm["id"].as_str().unwrap_or_default()).await;
    let rep = &r["result"]["suites"][0]["report"];
    assert!(rep["pairs"].as_u64().unwrap_or(0) >= 20 && rep["counterfactuals"].as_u64() == Some(4), "{r}");
    let ps = fx.post("/api/experiments/positional", json!({"lengths": [400]})).await;
    let r = wait_exp(&fx, ps["id"].as_str().unwrap_or_default()).await;
    assert_eq!(r["result"]["report"]["by_position"].as_object().map(|o| o.len()), Some(5), "{r}");
    // 🔒 ablation zonder bevestiging → geweigerd.
    let no = fx
        .http
        .post(format!("{}/api/experiments/ablation", fx.base))
        .json(&json!({}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(no.status(), 400);
    assert!(fx.get("/api/experiments").await.as_array().is_some_and(|a| a.len() == 4));
}

/// W17.06 (klein, per component): model weg / verbinding valt weg midden in de stream /
/// verminkte uitvoer → bericht netjes mislukt (geen hangende run); traag → gewoon klaar.
#[tokio::test]
async fn faults_model_connection_recovers_cleanly() {
    let fx = setup().await;
    let models = std::fs::read_to_string(fx.cfg.join("models.toml")).unwrap_or_default();
    let url = models
        .lines()
        .find_map(|l| l.strip_prefix("url = \""))
        .map(|u| u.trim_end_matches('"').trim_start_matches("http://").to_string())
        .unwrap_or_default();
    let proxy = vortex_core::faults::FaultProxy::start(&url).await.unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(fx.cfg.join("models.toml"), models.replace(&format!("http://{url}"), &proxy.url()))
        .unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let mut idx = 1;
    for (mode, expect) in [
        (vortex_core::faults::FaultMode::Refuse, "failed"),
        (vortex_core::faults::FaultMode::CutAfter(120), "failed"),
        (vortex_core::faults::FaultMode::Garble, "failed"),
        (vortex_core::faults::FaultMode::Delay(30), "complete"),
    ] {
        proxy.set(mode);
        fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": format!("fouttest {idx}"), "mode": "quick"}))
            .await;
        let mut status = String::new();
        for _ in 0..300 {
            let s = fx.get(&format!("/api/sessions/{sid}")).await;
            status = s["messages"][idx]["status"].as_str().unwrap_or("").to_string();
            if status != "streaming" && !status.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        // Weigeren/traag: exact; afkappen/verminken: netjes afgerond (mislukt of met deelantwoord), nooit hangend.
        let strict = matches!(mode, vortex_core::faults::FaultMode::Refuse | vortex_core::faults::FaultMode::Delay(_));
        assert!(status == expect || (!strict && status == "complete"), "{mode:?}: {status}");
        idx += 2;
    }
    // Geen hangende runs.
    let active = fx.get("/api/runs").await;
    assert!(active.as_array().is_none_or(|a| a.iter().all(|r| r["state"] != "running")), "{active}");
    proxy.stop();
}

/// W17.08: logprob-diagnostiek wijst het onzekere stuk aan (met alternatief).
#[tokio::test]
async fn logprob_diagnostics_flag_uncertain_tokens() {
    let fx = setup().await;
    let r = fx.post("/api/diagnostics/logprobs", json!({"prompt": "Wat is de hoofdstad van Frankrijk?"})).await;
    let d = &r["diagnosis"];
    assert_eq!(d["tokens"], 4, "{r}");
    assert_eq!(d["spans"][0]["text"], " Lyon", "{r}");
    assert_eq!(d["spans"][0]["alternatives"][0], " Parijs");
}

/// W17.14: browser-tool via de worker (echte Chromium) en repo-indexering.
#[tokio::test]
async fn browser_tool_and_repo_index() {
    let fx = setup().await;
    // Repo inlezen (eigen tools-crate als voorbeeld).
    let repo = fx._dir.path().join("mini");
    std::fs::create_dir_all(repo.join("src")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        repo.join("src/motor.rs"),
        "/// Start de motor van Vortex.\npub fn start_motor() -> u32 {\n    42\n}\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let r = fx.post("/api/repo/index", json!({"path": repo.display().to_string(), "name": "mini"})).await;
    assert_eq!(r["workspace"], "repo:mini", "{r}");
    let hits = fx.post("/api/memory/search", json!({"text": "start_motor functie", "workspace": "repo:mini"})).await;
    assert!(
        hits.as_array()
            .is_some_and(|h| h.iter().any(|x| x["memory"]["content"].as_str().unwrap_or("").contains("start_motor"))),
        "{hits}"
    );
    // Browser (alleen met worker + Chromium).
    if !std::path::Path::new("/opt/pw-browsers/chromium").exists() {
        return;
    }
    let Some(mut worker) = start_worker(&fx).await else { return };
    let app = axum::Router::new().route("/", axum::routing::get(|| async { axum::response::Html("<html><title>T</title><body><input id=q><button id=b onclick=\"document.getElementById('o').innerText='zocht: '+document.getElementById('q').value\">z</button><div id=o></div></body></html>") }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("{e}"));
    let url = format!("http://{}/", l.local_addr().unwrap_or_else(|e| panic!("{e}")));
    tokio::spawn(async move { axum::serve(l, app).await });
    let steps = json!([{"action": "goto", "url": url}, {"action": "fill", "selector": "#q", "text": "vortex"}, {"action": "click", "selector": "#b"}, {"action": "extract", "selector": "#o"}]);
    let res = fx.post("/api/tools/browser/call", json!({"input": {"steps": steps}})).await;
    assert_eq!(res["ok"], true, "{res}");
    assert!(res["output"]["text"].as_str().unwrap_or("").contains("zocht: vortex"), "{res}");
    let _ = worker.kill().await;
}

/// W17.15: MPO vs int8 op een eigen layer (router, met gelabelde HF-data) via de worker.
#[tokio::test]
async fn compression_experiment_on_own_layer() {
    let fx = setup().await;
    let Some(mut worker) = start_worker(&fx).await else { return };
    let hf = fx._dir.path().join("data/layers/hf");
    std::fs::create_dir_all(&hf).unwrap_or_else(|e| panic!("{e}"));
    let mut rows = String::new();
    for i in 0..90 {
        let (t, l) = match i % 3 {
            0 => (format!("Schrijf een Rust-functie die lijst {i} sorteert met code"), "code"),
            1 => (format!("Bereken {i} × 17 + 3 en geef het getal"), "math"),
            _ => (format!("hoi, hoe gaat het vandaag {i}?"), "chat"),
        };
        rows.push_str(&format!("{}\n", json!({"text": t, "label": l})));
    }
    std::fs::write(hf.join("router.jsonl"), rows).unwrap_or_else(|e| panic!("{e}"));
    let e = fx.post("/api/experiments/compression", json!({"layer": "router", "chis": [2, 4]})).await;
    let r = wait_exp(&fx, e["id"].as_str().unwrap_or_default()).await;
    assert_eq!(r["status"], "done", "{r}");
    let rows = r["result"]["result"]["rows"].as_array().cloned().unwrap_or_default();
    assert!(
        rows.iter().any(|x| x["variant"] == "int8") && rows.iter().any(|x| x["variant"] == "mpo χ=2 + healing"),
        "{r}"
    );
    // Alleen eigen layers: een "model" kan niet.
    let bad = fx.post("/api/experiments/compression", json!({"layer": "gpt-oss-20b"})).await;
    let rb = wait_exp(&fx, bad["id"].as_str().unwrap_or_default()).await;
    assert_eq!(rb["status"], "failed");
    let _ = worker.kill().await;
}

/// W17.17: cloud standaard uit; aan → model "cloud" gaat naar de externe dienst (met sleutel),
/// gelogd en begrensd per dag.
#[tokio::test]
async fn cloud_escalation_opt_in_logged_and_limited() {
    let fx = setup().await;
    let sid = fx.post("/api/sessions", json!({})).await["id"].as_str().unwrap_or_default().to_string();
    let off = fx
        .http
        .post(format!("{}/api/sessions/{sid}/messages", fx.base))
        .json(&json!({"text": "hoi", "model": "cloud"}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(off.status(), 400, "standaard uit");
    let models = std::fs::read_to_string(fx.cfg.join("models.toml")).unwrap_or_default();
    let url = models
        .lines()
        .find_map(|l| l.strip_prefix("url = \""))
        .map(|u| u.trim_end_matches('"').to_string())
        .unwrap_or_default();
    std::fs::write(
        fx.cfg.join("cloud.toml"),
        format!("enabled = true\nbase_url = \"{url}\"\nmodel = \"m\"\nmax_per_day = 1\n"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    fx.kernel.config().reload().unwrap_or_else(|e| panic!("{e}"));
    let nokey = fx
        .http
        .post(format!("{}/api/sessions/{sid}/messages", fx.base))
        .json(&json!({"text": "hoi", "model": "cloud"}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(nokey.status(), 400, "zonder sleutel niet");
    fx.kernel.secrets().set("cloud_api_key", "geheim").unwrap_or_else(|e| panic!("{e}"));
    let r = fx
        .post(
            &format!("/api/sessions/{sid}/messages"),
            json!({"text": "Leg kwantumverstrengeling uit", "model": "cloud", "mode": "quick"}),
        )
        .await;
    assert!(r["model"].as_str().unwrap_or("").starts_with("cloud:"), "{r}");
    fx.wait_message(&sid, 1, "complete").await;
    let logged = fx
        .kernel
        .events()
        .read_from(0, 100_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind.as_str() == "chat.cloud.escalated")
        .count();
    assert_eq!(logged, 1);
    let limit = fx
        .http
        .post(format!("{}/api/sessions/{sid}/messages", fx.base))
        .json(&json!({"text": "nog een", "model": "cloud"}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(limit.status(), 400, "daglimiet");
}

/// W18: causale effecten/counterfactual/simulatie op de hypotheses, assoc-experiment (MRR met vs
/// zonder), frontier-vlaggen standaard uit.
#[tokio::test]
async fn frontier_causal_assoc() {
    unsafe_env();
    let fx = setup().await;
    let st = fx.get("/api/frontier").await;
    assert_eq!(st["flags"]["assoc"], false, "alles standaard uit");
    let now = vortex_types::now().to_rfc3339();
    let _ = fx.kernel.events().db().write(|tx| {
        for (i, (c, e, p)) in [("renteverhoging", "minder hypotheken", 0.8), ("minder hypotheken", "dalende huizenprijzen", 0.6)].iter().enumerate() {
            tx.execute("INSERT INTO hypotheses(id, cause, effect, confidence, support, contra, last_seen) VALUES (?1,?2,?3,?4,2,0,?5)", rusqlite::params![format!("h{i}"), c, e, p, now])?;
        }
        Ok(())
    });
    let e = fx.post("/api/causal/effects", json!({"cause": "Renteverhoging"})).await;
    assert_eq!(e["effects"][0][0], "minder hypotheken", "{e}");
    let cf = fx.post("/api/causal/counterfactual", json!({"cause": "renteverhoging"})).await;
    assert!(
        cf["effects"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x["effect"] == "dalende huizenprijzen"
                && (x["difference"].as_f64().unwrap_or(0.0) - 0.48).abs() < 1e-6)),
        "{cf}"
    );
    let sim = fx.post("/api/world/simulate", json!({"on": ["renteverhoging"], "runs": 3000})).await;
    assert!((sim["frequencies"]["dalende huizenprijzen"].as_f64().unwrap_or(0.0) - 0.48).abs() < 0.04, "{sim}");
    // Associatief geheugen: experiment draait en meet (gate beslist op de uitkomst).
    let a = fx.post("/api/experiments/assoc", json!({})).await;
    let r = wait_exp(&fx, a["id"].as_str().unwrap_or_default()).await;
    assert_eq!(r["status"], "done", "{r}");
    assert!(
        r["result"]["mrr_zonder"].is_number()
            && r["result"]["mrr_met"].is_number()
            && r["result"]["advies"].is_string()
    );
    // Prompt-optimalisatie met een nep-model zonder kandidaten → netjes mislukt, niets toegepast.
    let p = fx.post("/api/experiments/prompt_opt", json!({"suites": ["redeneren"], "k": 1})).await;
    assert_eq!(wait_exp(&fx, p["id"].as_str().unwrap_or_default()).await["status"], "failed");
}

/// Mappen: ontbrekende map = geen crash (terugval) en aanpasbaar via de API/GUI.
#[tokio::test]
async fn paths_settings_missing_dirs_fall_back() {
    let fx = setup().await;
    let r = fx
        .http
        .put(format!("{}/api/settings/paths", fx.base))
        .json(&json!({"values": {"cold_dir": "Q:/bestaat-niet"}, "create": false}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.status(), 200);
    let p = fx.get("/api/settings/paths").await;
    let cold =
        p["items"].as_array().and_then(|a| a.iter().find(|i| i["key"] == "cold_dir").cloned()).unwrap_or_default();
    assert_eq!(cold["value"], "Q:/bestaat-niet");
    assert_eq!(cold["exists"], false);
    assert!(p["effective"]["cold_dir"].as_str().unwrap_or("").ends_with("cold"), "{p}");
    // Backup werkt gewoon (terugval op <data>/cold).
    assert!(fx.post("/api/autonomy/backups", json!({})).await["verified"] == true);
    let dir = fx._dir.path().join("koud");
    fx.http
        .put(format!("{}/api/settings/paths", fx.base))
        .json(&json!({"values": {"cold_dir": dir.display().to_string()}, "create": true}))
        .send()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(dir.is_dir(), "aangemaakt");
}
