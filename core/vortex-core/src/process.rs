//! Subprocess-supervisie: eigen procesgroep, outputlimieten, stoppen van de hele boom bij
//! annuleren, registratie als resource, events bij start/einde/afkappen.
// VX:W01.15

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, RunId, TraceId};

use crate::error::{CoreError, Result};
use crate::events::EventLog;
use crate::resources::{ResourceKind, ResourceRegistry};

/// Wat er gestart moet worden.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
    /// Maximaal aantal bytes per stroom (stdout/stderr); daarna afgekapt.
    pub max_output_bytes: usize,
    pub timeout: Option<Duration>,
    /// Wachttijd tussen netjes stoppen en hard doden.
    pub kill_grace: Duration,
    pub stdin: Option<Vec<u8>>,
    /// Harde geheugenlimiet (sandbox, W08.06): Unix via `ulimit -v`, Windows via Job Object.
    pub max_memory_bytes: Option<u64>,
    /// CPU-secondenlimiet (Unix `ulimit -t`; Windows: timeout geldt).
    pub max_cpu_secs: Option<u64>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            max_output_bytes: 1 << 20,
            timeout: None,
            kill_grace: Duration::from_millis(500),
            stdin: None,
            max_memory_bytes: None,
            max_cpu_secs: None,
        }
    }
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn args<I: IntoIterator<Item = S>, S: Into<String>>(mut self, a: I) -> Self {
        self.args.extend(a.into_iter().map(Into::into));
        self
    }
}

/// Hoe een proces eindigde.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProcessOutcome {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
}

impl ProcessOutcome {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.cancelled
    }
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

async fn read_limited<R: AsyncRead + Unpin>(mut r: R, max: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = max.saturating_sub(buf.len());
                if room > 0 {
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                }
                if n > room {
                    truncated = true; // blijf lezen (pijp leeg houden) maar bewaar niets meer
                }
            }
        }
    }
    (buf, truncated)
}

/// Supervisor van kindprocessen.
#[derive(Debug, Clone)]
pub struct Supervisor {
    log: EventLog,
    resources: ResourceRegistry,
}

impl Supervisor {
    pub fn new(log: EventLog, resources: ResourceRegistry) -> Self {
        Self { log, resources }
    }

    /// Start en wacht tot het proces klaar is, de timeout verloopt of `cancel` afgaat.
    pub async fn run(
        &self,
        spec: ProcessSpec,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> Result<ProcessOutcome> {
        let started = tokio::time::Instant::now();
        let mut cmd = limited_command(&spec);
        cmd.stdin(if spec.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        new_process_group(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| CoreError::Process(format!("{}: {e}", spec.program)))?;
        let pid = child.id().unwrap_or(0);
        #[cfg(windows)]
        let _job = spec.max_memory_bytes.and_then(|m| crate::platform::cap_memory(pid, m).ok());
        let _guard = self.resources.acquire(
            ResourceKind::Process,
            &spec.program,
            "supervisor",
            run,
            serde_json::json!({"pid": pid, "args": spec.args}),
        )?;
        let mut ev =
            Event::new(EventKind::ProcessSpawned, trace, serde_json::json!({"program": spec.program, "pid": pid}));
        ev.run_id = run;
        self.log.append(ev)?;

        if let (Some(input), Some(mut stdin)) = (spec.stdin.clone(), child.stdin.take()) {
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(&input).await;
            });
        }
        let out = child.stdout.take().map(|s| tokio::spawn(read_limited(s, spec.max_output_bytes)));
        let err = child.stderr.take().map(|s| tokio::spawn(read_limited(s, spec.max_output_bytes)));

        let timeout = async {
            match spec.timeout {
                Some(t) => tokio::time::sleep(t).await,
                None => std::future::pending().await,
            }
        };
        let (mut timed_out, mut cancelled) = (false, false);
        let status = tokio::select! {
            s = child.wait() => Some(s?),
            _ = timeout => { timed_out = true; None }
            _ = cancel.cancelled() => { cancelled = true; None }
        };
        let status = match status {
            Some(s) => s,
            None => terminate_tree(&mut child, spec.kill_grace).await?,
        };
        let (stdout, so_trunc) = match out {
            Some(h) => h.await.unwrap_or_default(),
            None => (Vec::new(), false),
        };
        let (stderr, se_trunc) = match err {
            Some(h) => h.await.unwrap_or_default(),
            None => (Vec::new(), false),
        };
        let outcome = ProcessOutcome {
            exit_code: status.code(),
            stdout,
            stderr,
            stdout_truncated: so_trunc,
            stderr_truncated: se_trunc,
            timed_out,
            cancelled,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        if so_trunc || se_trunc {
            let mut e = Event::new(
                EventKind::ProcessOutputTruncated,
                trace,
                serde_json::json!({"pid": pid, "limit": spec.max_output_bytes}),
            );
            e.run_id = run;
            self.log.append(e)?;
        }
        let mut e = Event::new(
            EventKind::ProcessExited,
            trace,
            serde_json::json!({
                "pid": pid, "exit_code": outcome.exit_code, "timed_out": timed_out,
                "cancelled": cancelled, "duration_ms": outcome.duration_ms
            }),
        );
        e.run_id = run;
        self.log.append(e)?;
        Ok(outcome)
    }
}

/// Command met limieten. Unix: `sh -c 'ulimit …; exec "$0" "$@"'` zodat de limiet het
/// programma zelf raakt (en kinderen erven hem).
fn limited_command(spec: &ProcessSpec) -> Command {
    #[cfg(unix)]
    if spec.max_memory_bytes.is_some() || spec.max_cpu_secs.is_some() {
        let mut script = String::new();
        if let Some(m) = spec.max_memory_bytes {
            script.push_str(&format!("ulimit -v {} 2>/dev/null; ", (m / 1024).max(1)));
        }
        if let Some(t) = spec.max_cpu_secs {
            script.push_str(&format!("ulimit -t {} 2>/dev/null; ", t.max(1)));
        }
        script.push_str("exec \"$0\" \"$@\"");
        let mut c = Command::new("sh");
        c.arg("-c").arg(script).arg(&spec.program).args(&spec.args);
        return c;
    }
    let mut c = Command::new(&spec.program);
    c.args(&spec.args);
    c
}

#[cfg(unix)]
fn new_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(windows)]
fn new_process_group(cmd: &mut Command) {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// Langlevend proces (bv. llama-server) onder supervisie. Stoppen met [`ServiceHandle::stop`];
/// bij drop wordt het proces gedood (kill_on_drop) en de resource vrijgegeven.
#[derive(Debug)]
pub struct ServiceHandle {
    child: Child,
    pid: u32,
    name: String,
    _resource: crate::resources::ResourceGuard,
    log: EventLog,
    trace: TraceId,
}

impl ServiceHandle {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Draait het proces nog?
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    /// Netjes stoppen (hele boom), na `grace` hard.
    pub async fn stop(mut self, grace: Duration) -> Result<Option<i32>> {
        let status =
            if self.is_running() { terminate_tree(&mut self.child, grace).await? } else { self.child.wait().await? };
        self.log.append(Event::new(
            EventKind::ProcessExited,
            self.trace,
            serde_json::json!({"pid": self.pid, "service": self.name, "exit_code": status.code()}),
        ))?;
        Ok(status.code())
    }
}

impl Supervisor {
    /// Start een langlevend proces. Uitvoer gaat naar `log_file` (append), anders weg.
    pub fn spawn_service(
        &self,
        spec: &ProcessSpec,
        owner: &str,
        log_file: Option<&std::path::Path>,
    ) -> Result<ServiceHandle> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args).stdin(Stdio::null()).kill_on_drop(true);
        match log_file {
            Some(p) => {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let f = std::fs::OpenOptions::new().create(true).append(true).open(p)?;
                let f2 = f.try_clone()?;
                cmd.stdout(Stdio::from(f)).stderr(Stdio::from(f2));
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        new_process_group(&mut cmd);
        let child = cmd.spawn().map_err(|e| CoreError::Process(format!("{}: {e}", spec.program)))?;
        let pid = child.id().unwrap_or(0);
        let res = self.resources.acquire(
            ResourceKind::Process,
            &spec.program,
            owner,
            None,
            serde_json::json!({"pid": pid, "args": spec.args, "service": true}),
        )?;
        let trace = TraceId::new();
        self.log.append(Event::new(
            EventKind::ProcessSpawned,
            trace,
            serde_json::json!({"program": spec.program, "pid": pid, "service": owner}),
        ))?;
        Ok(ServiceHandle { child, pid, name: owner.to_string(), _resource: res, log: self.log.clone(), trace })
    }
}

/// Stop de hele procesboom: eerst netjes, na `grace` hard.
async fn terminate_tree(child: &mut Child, grace: Duration) -> Result<std::process::ExitStatus> {
    let pid = child.id();
    #[cfg(unix)]
    if let Some(pid) = pid {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        let pg = Pid::from_raw(pid as i32);
        let _ = killpg(pg, Signal::SIGTERM);
        if let Ok(s) = tokio::time::timeout(grace, child.wait()).await {
            let _ = killpg(pg, Signal::SIGKILL); // eventuele kleinkinderen
            return Ok(s?);
        }
        let _ = killpg(pg, Signal::SIGKILL);
    }
    #[cfg(windows)]
    if let Some(pid) = pid {
        // Hele boom stoppen. (Job Objects met harde caps volgen in W04.)
        let _ = tokio::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        let _ = grace;
    }
    let _ = child.start_kill();
    Ok(child.wait().await?)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};
    use std::sync::Arc;

    fn sup() -> Result<(Supervisor, ResourceRegistry, EventLog)> {
        let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?));
        let res = ResourceRegistry::new(log.clone());
        Ok((Supervisor::new(log.clone(), res.clone()), res, log))
    }

    #[tokio::test]
    async fn captures_output_and_exit_code() -> Result<()> {
        let (s, res, _) = sup()?;
        let o = s
            .run(
                ProcessSpec::new("sh").args(["-c", "echo hallo; exit 3"]),
                None,
                TraceId::new(),
                CancellationToken::new(),
            )
            .await?;
        assert_eq!(o.exit_code, Some(3));
        assert_eq!(o.stdout_str().trim(), "hallo");
        assert!(res.held()?.is_empty(), "procesresource vrijgegeven");
        Ok(())
    }

    #[tokio::test]
    async fn output_is_limited() -> Result<()> {
        let (s, _, log) = sup()?;
        let mut spec = ProcessSpec::new("sh").args(["-c", "head -c 100000 /dev/zero"]);
        spec.max_output_bytes = 1000;
        let o = s.run(spec, None, TraceId::new(), CancellationToken::new()).await?;
        assert_eq!(o.stdout.len(), 1000);
        assert!(o.stdout_truncated);
        assert!(log.read_from(0, 100)?.iter().any(|e| e.kind == EventKind::ProcessOutputTruncated));
        Ok(())
    }

    #[tokio::test]
    async fn cancel_kills_whole_tree_quickly() -> Result<()> {
        let (s, _, _) = sup()?;
        let tok = CancellationToken::new();
        let t2 = tok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            t2.cancel();
        });
        let start = std::time::Instant::now();
        // kindproces start zelf een kleinkind dat SIGTERM negeert
        let o = s
            .run(ProcessSpec::new("sh").args(["-c", "trap '' TERM; sleep 30 & wait"]), None, TraceId::new(), tok)
            .await?;
        assert!(o.cancelled);
        assert!(start.elapsed() < Duration::from_secs(3), "stoppen duurde {:?}", start.elapsed());
        Ok(())
    }

    #[tokio::test]
    async fn timeout_stops_process() -> Result<()> {
        let (s, _, _) = sup()?;
        let mut spec = ProcessSpec::new("sleep").arg("30");
        spec.timeout = Some(Duration::from_millis(100));
        let o = s.run(spec, None, TraceId::new(), CancellationToken::new()).await?;
        assert!(o.timed_out);
        assert!(!o.success());
        Ok(())
    }

    #[tokio::test]
    async fn service_runs_until_stopped() -> Result<()> {
        let (s, res, _) = sup()?;
        let d = tempfile::tempdir()?;
        let log = d.path().join("svc.log");
        let mut h =
            s.spawn_service(&ProcessSpec::new("sh").args(["-c", "echo gestart; sleep 30"]), "svc", Some(&log))?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(h.is_running());
        assert_eq!(res.held()?.len(), 1);
        h.stop(Duration::from_millis(200)).await?;
        assert!(res.held()?.is_empty());
        assert!(std::fs::read_to_string(&log)?.contains("gestart"));
        Ok(())
    }

    #[tokio::test]
    async fn missing_program_is_error() -> Result<()> {
        let (s, _, _) = sup()?;
        assert!(
            s.run(ProcessSpec::new("bestaat-niet-xyz"), None, TraceId::new(), CancellationToken::new()).await.is_err()
        );
        Ok(())
    }
}
