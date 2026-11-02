//! W03 server-tests: chat end-to-end tegen een nep-model, hervatten van de eventstream,
//! stoppen midden in een stream, fork, instellingen met versies.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Json;
use axum::response::Response;
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
        let sys = b["messages"][0]["content"].as_str().unwrap_or("").to_string();
        let words: Vec<String> = if last == "slow" {
            (0..400).map(|i| format!("w{i} ")).collect()
        } else if last == "welke prompt?" {
            vec![sys]
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
    let state = vortex_server::api::build_state(kernel.clone());
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
    assert_eq!(fx.wait_message(&sid, 1, "complete").await["content"], "SYS-V1");
    let r = fx.put("/api/settings/prompts", json!({"path": "prompts/system.md", "content": "SYS-V2"})).await;
    assert_ne!(r["version"].as_str(), Some(v1.as_str()));
    fx.post(&format!("/api/sessions/{sid}/messages"), json!({"text": "welke prompt?"})).await;
    assert_eq!(fx.wait_message(&sid, 3, "complete").await["content"], "SYS-V2", "live vanaf volgend bericht");
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
