//! W01-gate: kill -9 midden in een run → herstart toont exact de juiste status en hervat.
//!
//! De test start zichzelf als kindproces (`crash_child_entry`), dat een kern opent, drie runs
//! in verschillende toestanden achterlaat en dan wacht. De ouder doodt het kind met SIGKILL
//! en opent de kern opnieuw.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use vortex_core::{EffectKind, Kernel, KernelOptions, Result, RunSpec, SecretBroker};
use vortex_types::{Priority, RunKind, RunState};

fn opts(dir: &std::path::Path) -> KernelOptions {
    let mut o = KernelOptions::new(dir.join("data"), dir.join("config"));
    o.secrets = SecretBroker::memory();
    o.watch_config = false;
    o
}

/// Kindproces-ingang. Niet zelfstandig draaien.
#[tokio::test]
#[ignore = "alleen als kindproces van crash_mid_run_recovers_exactly"]
async fn crash_child_entry() -> Result<()> {
    let Ok(dir) = std::env::var("VORTEX_CRASH_DIR") else { return Ok(()) };
    let dir = std::path::PathBuf::from(dir);
    let (k, _) = Kernel::open(opts(&dir)).await?;
    // 1. hervatbare run met checkpoint
    let mut s = RunSpec::new(RunKind::Ingest, Priority::P2);
    s.resumable = true;
    let a = k.start_run(s)?;
    a.start()?;
    a.checkpoint(&serde_json::json!({"pagina": 42}))?;
    // 2. run met een externe actie die "bezig" was
    let b = k.start_run(RunSpec::new(RunKind::Research, Priority::P1))?;
    b.start()?;
    k.checkpoints().effect_intent(b.id(), EffectKind::External, "POST naar webhook", Some("hook-1"))?;
    // 3. run die gestopt werd maar nog niet klaar was
    let c = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    c.start()?;
    k.request_stop(c.id())?;
    // 4. run in de wachtrij
    let q = k.start_run(RunSpec::new(RunKind::Eval, Priority::P3))?;
    // 5. voltooide run
    let done = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    done.start()?;
    done.complete(serde_json::json!("klaar"))?;
    println!("READY {} {} {} {}", a.id(), b.id(), c.id(), q.id());
    // contexten bewust lekken: geen nette afronding vóór de kill
    std::mem::forget((a, b, c, q));
    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn crash_mid_run_recovers_exactly() -> Result<()> {
    let d = tempfile::tempdir()?;
    std::fs::create_dir_all(d.path().join("config"))?;
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe)
        .args(["--exact", "crash_child_entry", "--ignored", "--nocapture", "--test-threads=1"])
        .env("VORTEX_CRASH_DIR", d.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("geen stdout"))?;
    let mut ids = None;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        if let Some(pos) = line.find("READY ") {
            let rest = &line[pos + "READY ".len()..];
            ids = Some(rest.split(' ').map(str::to_string).collect::<Vec<_>>());
            break;
        }
    }
    let ids = ids.ok_or_else(|| std::io::Error::other("kind meldde zich niet"))?;
    child.kill()?; // SIGKILL
    child.wait()?;

    let (k, rep) = Kernel::open(opts(d.path())).await?;
    let id = |i: usize| ids[i].parse().map_err(|_| std::io::Error::other("id"));
    let (a, b, c, q) = (id(0)?, id(1)?, id(2)?, id(3)?);

    assert!(rep.crashed, "crash moet herkend worden");
    assert_eq!(rep.was_running.len(), 2);
    assert_eq!(rep.resumable.iter().map(|r| r.id).collect::<Vec<_>>(), vec![a]);
    assert_eq!(rep.resumable[0].checkpoint_seq, Some(1));
    assert_eq!(rep.failed.iter().map(|r| r.id).collect::<Vec<_>>(), vec![b]);
    assert_eq!(rep.possibly_external.len(), 1);
    assert!(rep.needs_intervention.iter().any(|n| n.contains("POST naar webhook")));
    assert_eq!(rep.stopped.iter().map(|r| r.id).collect::<Vec<_>>(), vec![c]);
    assert_eq!(rep.still_queued, vec![q]);
    assert_eq!(rep.completed_recently.len(), 1);
    assert_eq!(k.runs().get(a)?.state, RunState::Queued);
    assert_eq!(k.runs().get(b)?.state, RunState::Failed);
    assert_eq!(k.runs().get(c)?.state, RunState::Cancelled);
    assert!(k.resources().held()?.is_empty());

    // hervatten vanaf checkpoint
    let (ctx, ckp) = k.resume_run(a)?;
    assert_eq!(ckp.map(|c| c.data["pagina"].clone()), Some(serde_json::json!(42)));
    ctx.start()?;
    let rec = ctx.complete(serde_json::json!({"hervat": true}))?;
    assert_eq!(rec.state, RunState::Completed);

    // nette afsluiting → volgende start is geen crash
    k.shutdown(Duration::from_secs(1)).await?;
    let (_k2, rep2) = Kernel::open(opts(d.path())).await?;
    assert!(!rep2.crashed);
    assert_eq!(rep2.summary(), "Nette start: niets te herstellen.");
    Ok(())
}
