//! Lifecycle van een model op een llama-server: gevonden → gevalideerd → laden → opwarmen
//! (readiness: tokenisatie, eerste generatie, protocol) → gereed → uitfaseren (draining) →
//! ontladen. Plus wisselen (met KV-cache save/restore), cache voorverwarmen en terug naar het
//! thuismodel na idle. Elke fase is een event.
// VX:W02.07

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use vortex_core::{EventLog, ProcessSpec, ServiceHandle, Supervisor};
use vortex_types::{Event, EventKind, TraceId};

use crate::backend::{Backend, Capabilities, ChatRequest, Message};
use crate::error::{ModelError, Result};
use crate::openai::{OpenAiBackend, ServerProps};
use crate::registry::ModelEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Discovered,
    Validated,
    Loading,
    WarmingUp,
    Ready,
    Draining,
    Unloaded,
    Failed,
}

/// Hoe de llama-server gestart wordt.
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Pad naar `llama-server` (of een test-dubbel).
    pub program: String,
    /// Extra programma-argumenten vóór de modelargumenten (bv. een script voor de test-dubbel).
    pub program_args: Vec<String>,
    pub host: String,
    pub port: u16,
    /// Fysieke GPU-index (`CUDA_VISIBLE_DEVICES`).
    pub gpu: Option<u32>,
    pub log_dir: PathBuf,
    pub health_timeout: Duration,
    pub drain_timeout: Duration,
    /// Map voor KV-cache save/restore (`--slot-save-path`).
    pub slot_save_dir: Option<PathBuf>,
}

/// Resultaat van laden (gemeten tijden gaan ook naar de registry/tracing).
#[derive(Debug, Clone, Serialize)]
pub struct LoadReport {
    pub model: String,
    pub load_ms: u64,
    pub warmup_ms: u64,
    pub n_ctx: Option<u64>,
    pub kv_restored: bool,
}

/// llama-server-argumenten uit een registry-entry.
pub fn llama_args(entry: &ModelEntry, host: &str, port: u16, slot_dir: Option<&Path>) -> Vec<String> {
    let mut a = vec!["-m".into(), entry.file.clone(), "--host".into(), host.into(), "--port".into(), port.to_string()];
    a.extend(["-ngl".into(), "999".into()]);
    if let Some(c) = entry.context {
        a.extend(["-c".into(), c.to_string()]);
    }
    if let Some(kv) = &entry.kv_type {
        a.extend(["--cache-type-k".into(), kv.clone(), "--cache-type-v".into(), kv.clone()]);
    }
    if entry.flash_attention.unwrap_or(true) {
        a.extend(["-fa".into(), "on".into()]);
    }
    let is = |r: &str| entry.roles.iter().any(|x| x == r);
    if is("embedding") {
        a.extend(["--embedding".into(), "--pooling".into(), "last".into()]);
    } else if is("rerank") {
        a.push("--reranking".into());
    } else {
        a.push("--jinja".into());
    }
    if let Some(d) = slot_dir {
        a.extend(["--slot-save-path".into(), d.to_string_lossy().into_owned()]);
    }
    // LoRA-adapters: laden zonder toe te passen; schaal zetten via /lora-adapters (hot-swap).
    if !entry.loras.is_empty() {
        for l in &entry.loras {
            a.extend(["--lora".into(), l.path.clone()]);
        }
        a.push("--lora-init-without-apply".into());
    }
    a.extend(entry.extra_args.iter().cloned());
    a
}

#[derive(Debug)]
struct State {
    phase: Phase,
    model: Option<String>,
    entry: Option<ModelEntry>,
    handle: Option<ServiceHandle>,
    backend: Option<Arc<OpenAiBackend>>,
    last_used: Instant,
}

/// Beheert één llama-server (één GPU).
#[derive(Debug, Clone)]
pub struct ModelServer {
    name: Arc<str>,
    cfg: LaunchConfig,
    sup: Supervisor,
    log: EventLog,
    /// Serialiseert laden/ontladen/wisselen (lang); `st` is alleen voor korte reads/writes.
    op: Arc<tokio::sync::Mutex<()>>,
    st: Arc<Mutex<State>>,
    inflight: Arc<AtomicUsize>,
    idle_notify: Arc<Notify>,
}

/// Houdt een verzoek "in vlucht" (draining wacht hierop).
#[derive(Debug)]
pub struct InflightGuard {
    inflight: Arc<AtomicUsize>,
    notify: Arc<Notify>,
    pub backend: Arc<OpenAiBackend>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if self.inflight.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.notify.notify_waiters();
        }
    }
}

impl ModelServer {
    pub fn new(name: &str, cfg: LaunchConfig, sup: Supervisor, log: EventLog) -> Self {
        Self {
            name: name.into(),
            cfg,
            sup,
            log,
            op: Arc::new(tokio::sync::Mutex::new(())),
            st: Arc::new(Mutex::new(State {
                phase: Phase::Unloaded,
                model: None,
                entry: None,
                handle: None,
                backend: None,
                last_used: Instant::now(),
            })),
            inflight: Arc::new(AtomicUsize::new(0)),
            idle_notify: Arc::new(Notify::new()),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.cfg.host, self.cfg.port)
    }

    pub async fn phase(&self) -> Phase {
        self.st.lock().phase
    }

    pub async fn current_model(&self) -> Option<String> {
        self.st.lock().model.clone()
    }

    fn event(&self, model: &str, from: Phase, to: Phase, extra: serde_json::Value) {
        let _ = self.log.append(Event::new(
            EventKind::ModelLifecycleChanged,
            TraceId::new(),
            json!({"server": &*self.name, "model": model, "from": from, "to": to, "detail": extra}),
        ));
    }

    fn set_phase(&self, model: &str, to: Phase, extra: serde_json::Value) {
        let from = {
            let mut st = self.st.lock();
            std::mem::replace(&mut st.phase, to)
        };
        self.event(model, from, to, extra);
    }

    /// Verzoek beginnen (alleen als `Ready`). Tijdens draining/laden: `Lifecycle`-fout → wachtrij bij de aanroeper.
    pub async fn begin_request(&self) -> Result<InflightGuard> {
        let mut st = self.st.lock();
        if st.phase != Phase::Ready {
            return Err(ModelError::Lifecycle(format!("server {} is {:?}, niet ready", self.name, st.phase)));
        }
        st.last_used = Instant::now();
        let backend = st.backend.clone().ok_or_else(|| ModelError::Lifecycle("geen backend".into()))?;
        self.inflight.fetch_add(1, Ordering::SeqCst);
        Ok(InflightGuard { inflight: self.inflight.clone(), notify: self.idle_notify.clone(), backend })
    }

    /// Laad `model` (ontlaadt eerst wat er draaide).
    pub async fn load(&self, model: &str, entry: &ModelEntry) -> Result<LoadReport> {
        let _op = self.op.lock().await;
        self.unload_locked().await?;
        self.set_phase(model, Phase::Discovered, json!({"file": entry.file}));
        // Gevalideerd: bestand bestaat, parser bekend.
        if !Path::new(&entry.file).exists() {
            self.set_phase(model, Phase::Failed, json!({"reden": "bestand ontbreekt"}));
            return Err(ModelError::Lifecycle(format!("modelbestand ontbreekt: {}", entry.file)));
        }
        crate::toolcall::parser_by_id(&entry.toolcall_parser)?;
        self.set_phase(model, Phase::Validated, json!({}));

        // Laden.
        let t0 = Instant::now();
        let mut spec = ProcessSpec::new(&self.cfg.program);
        spec.args = self.cfg.program_args.clone();
        spec.args.extend(llama_args(entry, &self.cfg.host, self.cfg.port, self.cfg.slot_save_dir.as_deref()));
        if let Some(g) = self.cfg.gpu {
            spec.env.push(("CUDA_VISIBLE_DEVICES".into(), g.to_string()));
        }
        self.set_phase(model, Phase::Loading, json!({"args": spec.args}));
        let log_file = self.cfg.log_dir.join(format!("{}.log", self.name));
        let mut handle = self.sup.spawn_service(&spec, &format!("model-server:{}", self.name), Some(&log_file))?;
        let caps = entry.capabilities.clone().unwrap_or_else(|| default_caps(entry));
        let mut backend = OpenAiBackend::new(&self.name, &self.url(), model, &caps, &entry.toolcall_parser)?;
        let deadline = Instant::now() + self.cfg.health_timeout;
        loop {
            if !handle.is_running() {
                let _ = handle.stop(Duration::ZERO).await;
                self.set_phase(model, Phase::Failed, json!({"reden": "proces stopte tijdens laden", "log": log_file}));
                return Err(ModelError::Lifecycle(format!(
                    "llama-server stopte tijdens laden; zie {}",
                    log_file.display()
                )));
            }
            if backend.health().await.unwrap_or(false) {
                break;
            }
            if Instant::now() > deadline {
                let _ = handle.stop(Duration::from_millis(500)).await;
                self.set_phase(model, Phase::Failed, json!({"reden": "health-timeout"}));
                return Err(ModelError::Lifecycle(format!(
                    "{} niet gezond binnen {:?}",
                    model, self.cfg.health_timeout
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let load_ms = t0.elapsed().as_millis() as u64;

        // Opwarmen + readiness.
        self.set_phase(model, Phase::WarmingUp, json!({"load_ms": load_ms}));
        let t1 = Instant::now();
        let props = match self.readiness(&mut backend, entry).await {
            Ok(p) => p,
            Err(e) => {
                let _ = handle.stop(Duration::from_millis(500)).await;
                self.set_phase(model, Phase::Failed, json!({"reden": e.to_string()}));
                return Err(e);
            }
        };
        let warmup_ms = t1.elapsed().as_millis() as u64;
        let kv_restored = self.restore_kv(model).await;
        let mut st = self.st.lock();
        st.handle = Some(handle);
        st.backend = Some(Arc::new(backend));
        st.model = Some(model.into());
        st.entry = Some(entry.clone());
        st.last_used = Instant::now();
        drop(st);
        self.set_phase(
            model,
            Phase::Ready,
            json!({"load_ms": load_ms, "warmup_ms": warmup_ms, "kv_restored": kv_restored}),
        );
        Ok(LoadReport { model: model.into(), load_ms, warmup_ms, n_ctx: props.n_ctx, kv_restored })
    }

    async fn readiness(&self, b: &mut OpenAiBackend, entry: &ModelEntry) -> Result<ServerProps> {
        let tok = CancellationToken::new();
        let is = |r: &str| entry.roles.iter().any(|x| x == r);
        if is("embedding") {
            let e = b.embed(&["readiness".into()], &tok).await?;
            if let Some(d) = entry.dims
                && e.first().map(|v| v.len()) < Some(d as usize)
            {
                return Err(ModelError::Lifecycle(format!("embedding-dimensie {} < verwacht {d}", e[0].len())));
            }
            return Ok(ServerProps::default());
        }
        if is("rerank") {
            b.rerank("readiness", &["a".into(), "b".into()], &tok).await?;
            return Ok(ServerProps::default());
        }
        // Tokenisatie.
        let client = reqwest::Client::new();
        let t: serde_json::Value = client
            .post(format!("{}/tokenize", self.url()))
            .json(&json!({"content": "Vortex readiness"}))
            .send()
            .await?
            .json()
            .await?;
        if t.get("tokens").and_then(|x| x.as_array()).is_none_or(|a| a.is_empty()) {
            return Err(ModelError::Lifecycle("tokenisatie gaf geen tokens".into()));
        }
        // Protocol (template-contract) + eerste generatie.
        let props = b.probe().await?;
        let mut req = ChatRequest::new(vec![Message::user("Zeg ok.")]);
        req.max_tokens = Some(4);
        b.chat(&req, &tok).await?;
        Ok(props)
    }

    /// Uitfaseren: geen nieuwe verzoeken, wacht op lopende, stop proces.
    pub async fn unload(&self) -> Result<()> {
        let _op = self.op.lock().await;
        self.unload_locked().await
    }

    async fn unload_locked(&self) -> Result<()> {
        let Some(model) = self.st.lock().model.clone() else { return Ok(()) };
        self.set_phase(&model, Phase::Draining, json!({"inflight": self.inflight.load(Ordering::SeqCst)}));
        let deadline = tokio::time::Instant::now() + self.cfg.drain_timeout;
        loop {
            let n = self.idle_notify.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.inflight.load(Ordering::SeqCst) == 0 {
                break;
            }
            if tokio::time::timeout_at(deadline, n).await.is_err() {
                tracing::warn!(server = %self.name, "draining-timeout; lopende verzoeken worden afgebroken");
                break;
            }
        }
        let handle = {
            let mut st = self.st.lock();
            st.backend = None;
            st.model = None;
            st.entry = None;
            st.handle.take()
        };
        if let Some(h) = handle {
            h.stop(Duration::from_secs(2)).await?;
        }
        self.set_phase(&model, Phase::Unloaded, json!({}));
        Ok(())
    }

    // VX:W02.09
    async fn save_kv(&self, model: &str) -> bool {
        let Some(dir) = &self.cfg.slot_save_dir else { return false };
        let _ = std::fs::create_dir_all(dir);
        let r = reqwest::Client::new()
            .post(format!("{}/slots/0?action=save", self.url()))
            .json(&json!({"filename": format!("{model}.bin")}))
            .send()
            .await;
        matches!(r, Ok(r) if r.status().is_success())
    }

    async fn restore_kv(&self, model: &str) -> bool {
        let Some(dir) = &self.cfg.slot_save_dir else { return false };
        if !dir.join(format!("{model}.bin")).exists() {
            return false;
        }
        let r = reqwest::Client::new()
            .post(format!("{}/slots/0?action=restore", self.url()))
            .json(&json!({"filename": format!("{model}.bin")}))
            .send()
            .await;
        matches!(r, Ok(r) if r.status().is_success())
    }

    /// Wissel naar een ander model: KV-cache van het huidige opslaan, uitfaseren, nieuw laden
    /// (en diens KV-cache herstellen als die er is).
    // VX:W02.08
    pub async fn switch(&self, to: &str, entry: &ModelEntry) -> Result<LoadReport> {
        if self.current_model().await.as_deref() == Some(to) && self.phase().await == Phase::Ready {
            return Ok(LoadReport { model: to.into(), load_ms: 0, warmup_ms: 0, n_ctx: None, kv_restored: false });
        }
        let t0 = Instant::now();
        if let Some(cur) = self.current_model().await {
            let saved = self.save_kv(&cur).await;
            self.event(&cur, Phase::Ready, Phase::Ready, json!({"kv_saved": saved}));
        }
        let rep = self.load(to, entry).await?;
        self.event(to, Phase::Ready, Phase::Ready, json!({"switch_ms": t0.elapsed().as_millis() as u64}));
        Ok(rep)
    }

    /// Na `idle` zonder gebruik terug naar het thuismodel. Draait tot `stop`.
    pub fn spawn_idle_return(
        &self,
        home: String,
        home_entry: ModelEntry,
        idle: Duration,
        stop: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = tokio::time::sleep(idle / 4) => {}
                }
                let (cur, last, phase) = {
                    let st = me.st.lock();
                    (st.model.clone(), st.last_used, st.phase)
                };
                if phase == Phase::Ready
                    && cur.as_deref() != Some(home.as_str())
                    && last.elapsed() >= idle
                    && me.inflight.load(Ordering::SeqCst) == 0
                    && let Err(e) = me.switch(&home, &home_entry).await
                {
                    tracing::warn!(error = %e, "terug naar thuismodel mislukt");
                }
            }
        })
    }
}

fn default_caps(entry: &ModelEntry) -> Capabilities {
    let is = |r: &str| entry.roles.iter().any(|x| x == r);
    let chat = !(is("embedding") || is("rerank"));
    Capabilities {
        chat,
        streaming: chat,
        tools: chat && (is("tools") || is("agents")),
        vision: is("vision") || is("ocr"),
        embeddings: is("embedding"),
        rerank: is("rerank"),
        cancellation: true,
        logprobs: chat,
        adapters: false,
        json_schema: chat,
        grammar: chat,
        kv_slots: chat,
        reasoning_effort: chat && entry.file.to_lowercase().contains("gpt-oss"),
    }
}

/// Modelbestand alvast in de OS-cache lezen (voorspellend laden: volgende wissel sneller).
/// Draait op de blocking-pool; geeft gelezen bytes terug.
pub async fn prefetch_file(path: PathBuf, cancel: CancellationToken) -> Result<u64> {
    tokio::task::spawn_blocking(move || -> Result<u64> {
        use std::io::Read;
        let mut f =
            std::fs::File::open(&path).map_err(|e| ModelError::Lifecycle(format!("{}: {e}", path.display())))?;
        let mut buf = vec![0u8; 16 << 20];
        let mut total = 0u64;
        loop {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            let n = f.read(&mut buf).map_err(|e| ModelError::Lifecycle(e.to_string()))?;
            if n == 0 {
                return Ok(total);
            }
            total += n as u64;
        }
    })
    .await
    .map_err(|e| ModelError::Lifecycle(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_follow_roles() {
        let e = ModelEntry {
            file: "m.gguf".into(),
            roles: vec!["chat".into()],
            gpu: "main".into(),
            context: Some(8192),
            kv_type: Some("q8_0".into()),
            flash_attention: None,
            vram_weights_mib: None,
            chat_template: None,
            toolcall_parser: "openai-native/v1".into(),
            capabilities: None,
            load_ms_measured: None,
            license: None,
            dims: None,
            extra_args: vec!["--threads".into(), "8".into()],
            loras: vec![crate::registry::LoraEntry { name: "stijl".into(), path: "a.gguf".into(), scale: 0.0 }],
        };
        let a = llama_args(&e, "127.0.0.1", 8080, Some(Path::new("/s")));
        assert!(a.join(" ").contains("--lora a.gguf --lora-init-without-apply"));
        let s = a.join(" ");
        for frag in [
            "-m m.gguf",
            "--port 8080",
            "-c 8192",
            "--cache-type-k q8_0",
            "-fa on",
            "--jinja",
            "--slot-save-path /s",
            "--threads 8",
        ] {
            assert!(s.contains(frag), "{frag} in {s}");
        }
        let mut emb = e.clone();
        emb.roles = vec!["embedding".into()];
        assert!(llama_args(&emb, "h", 1, None).contains(&"--embedding".to_string()));
        assert!(default_caps(&emb).embeddings && !default_caps(&emb).chat);
    }
}
