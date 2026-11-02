//! Sandbox-procesgroep (W08.06): Python en shell in een eigen procesgroep met limieten (tijd,
//! geheugen, CPU, uitvoer), eigen werkmap, schone omgeving; annuleren stopt de hele boom.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use vortex_core::{ProcessOutcome, ProcessSpec, Supervisor};
use vortex_types::{RunId, TraceId};

use crate::error::{ToolError, ToolErrorKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub timeout_ms: u64,
    pub max_memory_mb: u64,
    pub max_cpu_secs: u64,
    pub max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { timeout_ms: 60_000, max_memory_mb: 2048, max_cpu_secs: 120, max_output_bytes: 256 * 1024 }
    }
}

#[derive(Debug, Clone)]
pub struct Sandbox {
    supervisor: Supervisor,
    /// Werkmap voor code (scratch); scripts en uitvoerbestanden komen hier.
    pub workdir: PathBuf,
    pub python: String,
    pub limits: Limits,
}

/// Resultaat in een vorm die het model goed leest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
    pub duration_ms: u64,
}

impl RunReport {
    fn from(o: ProcessOutcome) -> Self {
        Self {
            exit_code: o.exit_code,
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            timed_out: o.timed_out,
            truncated: o.stdout_truncated || o.stderr_truncated,
            duration_ms: o.duration_ms,
        }
    }
    pub fn text(&self) -> String {
        let mut t = String::new();
        if !self.stdout.trim().is_empty() {
            t.push_str(self.stdout.trim_end());
        }
        if !self.stderr.trim().is_empty() {
            if !t.is_empty() {
                t.push('\n');
            }
            t.push_str("[stderr]\n");
            t.push_str(self.stderr.trim_end());
        }
        if self.timed_out {
            t.push_str("\n[afgebroken: tijdslimiet]");
        }
        if self.truncated {
            t.push_str("\n[uitvoer afgekapt]");
        }
        if t.is_empty() {
            t = format!("(geen uitvoer, exitcode {})", self.exit_code.map_or("?".into(), |c| c.to_string()));
        }
        t
    }
}

pub fn default_python() -> String {
    if cfg!(windows) { "python".into() } else { "python3".into() }
}

impl Sandbox {
    pub fn new(supervisor: Supervisor, workdir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&workdir);
        Self { supervisor, workdir, python: default_python(), limits: Limits::default() }
    }

    fn spec(&self, program: &str, args: Vec<String>, cwd: Option<PathBuf>, limits: &Limits) -> ProcessSpec {
        let mut s = ProcessSpec::new(program).args(args);
        s.cwd = Some(cwd.unwrap_or_else(|| self.workdir.clone()));
        s.timeout = Some(Duration::from_millis(limits.timeout_ms));
        s.max_output_bytes = limits.max_output_bytes;
        s.max_memory_bytes = Some(limits.max_memory_mb * 1024 * 1024);
        s.max_cpu_secs = Some(limits.max_cpu_secs);
        s.env = vec![
            ("PYTHONIOENCODING".into(), "utf-8".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
            ("MPLBACKEND".into(), "Agg".into()),
        ];
        s
    }

    async fn go(
        &self,
        spec: ProcessSpec,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> Result<RunReport, ToolError> {
        let o = self
            .supervisor
            .run(spec, run, trace, cancel)
            .await
            .map_err(|e| ToolError::new(ToolErrorKind::Failed, e.to_string()))?;
        if o.cancelled {
            return Err(ToolError::new(ToolErrorKind::Cancelled, "geannuleerd"));
        }
        Ok(RunReport::from(o))
    }

    /// Python-code uitvoeren (`-I`: geïsoleerd, geen user-site, geen cwd in sys.path).
    pub async fn python(
        &self,
        code: &str,
        limits: Option<Limits>,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> Result<RunReport, ToolError> {
        let l = limits.unwrap_or_else(|| self.limits.clone());
        let script = self.workdir.join(format!("_vx_{}.py", vortex_types::EventId::new()));
        std::fs::write(&script, code)?;
        let r = self
            .go(self.spec(&self.python, vec!["-I".into(), script.display().to_string()], None, &l), run, trace, cancel)
            .await;
        let _ = std::fs::remove_file(&script);
        r
    }

    /// Script-bestand uitvoeren (bv. uit een skill) met argumenten.
    pub async fn script(
        &self,
        path: &std::path::Path,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> Result<RunReport, ToolError> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let (prog, mut a): (String, Vec<String>) = match ext {
            "py" => (self.python.clone(), vec!["-I".into(), path.display().to_string()]),
            "sh" => ("sh".into(), vec![path.display().to_string()]),
            "ps1" => ("powershell".into(), vec!["-NoProfile".into(), "-File".into(), path.display().to_string()]),
            "js" | "mjs" => ("node".into(), vec![path.display().to_string()]),
            _ => (path.display().to_string(), vec![]),
        };
        a.extend(args);
        let l = self.limits.clone();
        self.go(self.spec(&prog, a, cwd, &l), run, trace, cancel).await
    }

    /// Shell-commando (Unix: sh -c; Windows: PowerShell).
    pub async fn shell(
        &self,
        command: &str,
        cwd: Option<PathBuf>,
        limits: Option<Limits>,
        run: Option<RunId>,
        trace: TraceId,
        cancel: CancellationToken,
    ) -> Result<RunReport, ToolError> {
        let l = limits.unwrap_or_else(|| self.limits.clone());
        let (prog, args) = if cfg!(windows) {
            (
                "powershell".to_string(),
                vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), command.to_string()],
            )
        } else {
            ("sh".to_string(), vec!["-c".into(), command.to_string()])
        };
        self.go(self.spec(&prog, args, cwd, &l), run, trace, cancel).await
    }
}
