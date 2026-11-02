//! Lifecycle-tests met een test-dubbel van llama-server (Python, alleen stdlib).
//! Echte modellen + metingen: op de PC (W02.10–W02.16).
#![cfg(unix)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use vortex_core::{Db, EventLog, ResourceRegistry, Supervisor};
use vortex_models::backend::{Backend, ChatRequest, Message};
use vortex_models::{LaunchConfig, ModelEntry, ModelError, ModelServer, Phase};
use vortex_types::EventKind;

type R = Result<(), ModelError>;

fn python() -> Option<String> {
    ["python3", "python"]
        .into_iter()
        .find(|p| std::process::Command::new(p).arg("--version").output().is_ok())
        .map(String::from)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(18080)
}

struct Fx {
    _dir: tempfile::TempDir,
    root: PathBuf,
    srv: ModelServer,
    log: EventLog,
    res: ResourceRegistry,
}

fn setup() -> Option<Fx> {
    let py = python()?;
    let dir = tempfile::tempdir().ok()?;
    let root = dir.path().to_path_buf();
    let db = Arc::new(Db::open_in_memory(vortex_core::db::CORE_MIGRATIONS).ok()?);
    let log = EventLog::new(db);
    let res = ResourceRegistry::new(log.clone());
    let sup = Supervisor::new(log.clone(), res.clone());
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_llama_server.py");
    let cfg = LaunchConfig {
        program: py,
        program_args: vec![script.to_string_lossy().into_owned()],
        host: "127.0.0.1".into(),
        port: free_port(),
        gpu: Some(0),
        log_dir: root.join("logs"),
        health_timeout: Duration::from_secs(10),
        drain_timeout: Duration::from_secs(5),
        slot_save_dir: Some(root.join("slots")),
    };
    Some(Fx { srv: ModelServer::new("main", cfg, sup, log.clone()), _dir: dir, root, log, res })
}

fn entry(fx: &Fx, name: &str, content: &str, roles: &[&str]) -> ModelEntry {
    let file = fx.root.join(format!("{name}.gguf"));
    let _ = std::fs::write(&file, content);
    ModelEntry {
        file: file.to_string_lossy().into_owned(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
        gpu: "main".into(),
        context: Some(4096),
        kv_type: Some("q8_0".into()),
        flash_attention: Some(true),
        vram_weights_mib: None,
        chat_template: None,
        toolcall_parser: "openai-native/v1".into(),
        capabilities: None,
        load_ms_measured: None,
        license: None,
        dims: if roles.contains(&"embedding") { Some(512) } else { None },
        extra_args: vec![],
    }
}

#[tokio::test]
async fn load_warmup_ready_unload_with_events() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let e = entry(&fx, "a", "ok", &["chat"]);
    let rep = fx.srv.load("a", &e).await?;
    assert_eq!(fx.srv.phase().await, Phase::Ready);
    assert!(rep.load_ms >= 300, "health wachtte op laden ({} ms)", rep.load_ms);
    assert_eq!(rep.n_ctx, Some(4096));
    let g = fx.srv.begin_request().await?;
    let r = g.backend.chat(&ChatRequest::new(vec![Message::user("hoi")]), &Default::default()).await?;
    assert_eq!(r.content, "ok a.gguf");
    drop(g);
    fx.srv.unload().await?;
    assert_eq!(fx.srv.phase().await, Phase::Unloaded);
    assert!(fx.res.held()?.is_empty(), "proces opgeruimd");
    let phases: Vec<String> = fx
        .log
        .read_from(0, 500)?
        .into_iter()
        .filter(|e| e.kind == EventKind::ModelLifecycleChanged)
        .map(|e| e.payload["to"].as_str().unwrap_or_default().to_string())
        .collect();
    let want = ["discovered", "validated", "loading", "warming_up", "ready", "draining", "unloaded"];
    let mut it = phases.iter();
    for w in want {
        assert!(it.any(|p| p == w), "fase {w} ontbreekt in {phases:?}");
    }
    Ok(())
}

#[tokio::test]
async fn missing_file_and_crash_on_load_are_failed_not_hanging() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let mut e = entry(&fx, "x", "ok", &["chat"]);
    e.file = fx.root.join("bestaat-niet.gguf").to_string_lossy().into_owned();
    assert!(fx.srv.load("x", &e).await.is_err());
    let c = entry(&fx, "c", "CRASH", &["chat"]);
    let t0 = std::time::Instant::now();
    assert!(matches!(fx.srv.load("c", &c).await, Err(ModelError::Lifecycle(_))));
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(fx.srv.phase().await, Phase::Failed);
    assert!(fx.res.held()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn draining_waits_for_inflight_and_refuses_new() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let e = entry(&fx, "a", "ok", &["chat"]);
    fx.srv.load("a", &e).await?;
    let g = fx.srv.begin_request().await?;
    let slow = tokio::spawn(async move {
        let r = g.backend.chat(&ChatRequest::new(vec![Message::user("slow")]), &Default::default()).await;
        drop(g);
        r.map(|r| r.content)
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let srv = fx.srv.clone();
    let unload = tokio::spawn(async move { srv.unload().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fx.srv.phase().await, Phase::Draining);
    assert!(fx.srv.begin_request().await.is_err(), "geen nieuwe verzoeken tijdens draining");
    let content = slow.await.map_err(|e| ModelError::Lifecycle(e.to_string()))??;
    assert_eq!(content, "ok a.gguf", "lopend verzoek maakt zijn werk af");
    unload.await.map_err(|e| ModelError::Lifecycle(e.to_string()))??;
    assert_eq!(fx.srv.phase().await, Phase::Unloaded);
    Ok(())
}

#[tokio::test]
async fn switch_saves_and_restores_kv() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let a = entry(&fx, "a", "ok", &["chat"]);
    let b = entry(&fx, "b", "ok", &["chat"]);
    fx.srv.load("a", &a).await?;
    let r1 = fx.srv.switch("b", &b).await?;
    assert_eq!(fx.srv.current_model().await.as_deref(), Some("b"));
    assert!(!r1.kv_restored, "b had nog geen KV-cache");
    assert!(fx.root.join("slots/a.bin").exists(), "KV van a opgeslagen");
    let r2 = fx.srv.switch("a", &a).await?;
    assert!(r2.kv_restored, "KV van a hersteld");
    let same = fx.srv.switch("a", &a).await?;
    assert_eq!(same.load_ms, 0, "al geladen = niets doen");
    fx.srv.unload().await?;
    Ok(())
}

#[tokio::test]
async fn idle_returns_to_home_model() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let home = entry(&fx, "home", "ok", &["chat"]);
    let code = entry(&fx, "code", "ok", &["chat", "code"]);
    fx.srv.load("code", &code).await?;
    let stop = tokio_util::sync::CancellationToken::new();
    let h = fx.srv.spawn_idle_return("home".into(), home, Duration::from_millis(400), stop.clone());
    let mut ok = false;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if fx.srv.current_model().await.as_deref() == Some("home") && fx.srv.phase().await == Phase::Ready {
            ok = true;
            break;
        }
    }
    stop.cancel();
    let _ = h.await;
    assert!(ok, "na idle terug naar thuismodel");
    fx.srv.unload().await?;
    Ok(())
}

#[tokio::test]
async fn embedding_readiness_checks_dims() -> R {
    let Some(fx) = setup() else { return Ok(()) };
    let mut e = entry(&fx, "emb", "ok", &["embedding"]);
    fx.srv.load("emb", &e).await?;
    fx.srv.unload().await?;
    e.dims = Some(4096);
    assert!(fx.srv.load("emb", &e).await.is_err(), "te kleine dimensie = niet ready");
    Ok(())
}

#[tokio::test]
async fn prefetch_reads_whole_file() -> R {
    let d = tempfile::tempdir().map_err(|e| ModelError::Lifecycle(e.to_string()))?;
    let p = d.path().join("m.gguf");
    std::fs::write(&p, vec![7u8; 40 << 20]).map_err(|e| ModelError::Lifecycle(e.to_string()))?;
    let n = vortex_models::lifecycle::prefetch_file(p, Default::default()).await?;
    assert_eq!(n, 40 << 20);
    Ok(())
}
