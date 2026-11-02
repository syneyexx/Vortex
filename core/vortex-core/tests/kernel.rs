//! Integratietests van de kern: levenscyclus, stopketen, budget, config-versie per run.

use std::time::{Duration, Instant};

use vortex_core::{Budget, CoreError, Kernel, KernelOptions, ProcessSpec, Result, RunSpec, SecretBroker};
use vortex_types::{EventKind, Priority, RunKind, RunState, StopStatus};

async fn kernel(dir: &std::path::Path) -> Result<Kernel> {
    std::fs::create_dir_all(dir.join("config"))?;
    std::fs::write(dir.join("config/models.toml"), "default = \"a\"\n")?;
    let mut o = KernelOptions::new(dir.join("data"), dir.join("config"));
    o.secrets = SecretBroker::memory();
    o.watch_config = false;
    Ok(Kernel::open(o).await?.0)
}

#[tokio::test]
async fn full_lifecycle_is_logged() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let ctx = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    let id = ctx.id();
    ctx.start()?;
    ctx.step()?;
    ctx.charge_tokens(10)?;
    ctx.checkpoint(&serde_json::json!({"stap": 1}))?;
    let rec = ctx.complete(serde_json::json!({"antwoord": "hoi"}))?;
    assert_eq!(rec.state, RunState::Completed);
    assert_eq!(rec.usage.tokens, 10);
    let kinds: Vec<_> = k.events().for_run(id)?.into_iter().map(|e| e.kind).collect();
    assert_eq!(kinds.first(), Some(&EventKind::RunCreated));
    assert!(kinds.contains(&EventKind::RunCheckpointed));
    assert!(k.resources().held()?.is_empty());
    k.shutdown(Duration::from_secs(1)).await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn stop_propagates_through_child_run_and_process_under_500ms() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let parent = k.start_run(RunSpec::new(RunKind::Research, Priority::P1))?;
    parent.start()?;
    let child = parent.child(RunSpec::new(RunKind::Other("tool".into()), Priority::P1), Budget::default())?;
    child.start()?;
    let (pid, cid) = (parent.id(), child.id());
    let k2 = k.clone();
    let worker = tokio::spawn(async move {
        let out = child.spawn_process(ProcessSpec::new("sleep").arg("30")).await;
        let cancelled = out.as_ref().map(|o| o.cancelled).unwrap_or(false);
        let rec = child.finish_cancelled();
        (cancelled, rec.map(|r| r.state))
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let t0 = Instant::now();
    assert!(k2.request_stop(pid)?);
    let (cancelled, child_state) = worker.await.map_err(|e| CoreError::Process(e.to_string()))?;
    let stop_latency = t0.elapsed();
    assert!(cancelled, "proces moet door de stop gestopt zijn");
    assert_eq!(child_state?, RunState::Cancelled);
    let prec = parent.finish_cancelled()?;
    assert_eq!(prec.state, RunState::Cancelled);
    assert_eq!(prec.stop_status, Some(StopStatus::CleanedUp));
    assert!(stop_latency < Duration::from_millis(500), "stoplatency {stop_latency:?}");
    assert_eq!(k.runs().get(cid)?.stop_status, Some(StopStatus::CleanedUp));
    assert!(k.resources().held()?.is_empty(), "geen achtergebleven resources");
    Ok(())
}

#[tokio::test]
async fn stop_while_queued_cancels_directly() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let ctx = k.start_run(RunSpec::new(RunKind::Ingest, Priority::P2))?;
    k.request_stop(ctx.id())?;
    assert!(matches!(ctx.start(), Err(CoreError::Cancelled)));
    let r = ctx.finish_cancelled()?;
    assert_eq!(r.state, RunState::Cancelled);
    Ok(())
}

#[tokio::test]
async fn budget_exceeded_is_error_and_event() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let mut spec = RunSpec::new(RunKind::Chat, Priority::P0);
    spec.budget.max_steps = Some(1);
    let ctx = k.start_run(spec)?;
    ctx.start()?;
    ctx.step()?;
    assert!(matches!(ctx.step(), Err(CoreError::BudgetExceeded(_))));
    let id = ctx.id();
    ctx.fail("budget op")?;
    assert!(k.events().for_run(id)?.iter().any(|e| e.kind == EventKind::RunBudgetExceeded));
    Ok(())
}

#[tokio::test]
async fn dropped_context_never_leaves_run_hanging() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let ctx = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    let id = ctx.id();
    ctx.start()?;
    drop(ctx);
    let r = k.runs().get(id)?;
    assert_eq!(r.state, RunState::Failed);
    assert!(r.error.unwrap_or_default().contains("zonder afronding"));
    Ok(())
}

#[tokio::test]
async fn run_keeps_its_config_version() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let mut ctx = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    let v1 = ctx.config().version.clone();
    std::fs::write(d.path().join("config/models.toml"), "default = \"b\"\n")?;
    assert!(k.config().reload()?);
    assert_eq!(ctx.config().version, v1, "lopende run houdt eigen versie");
    assert_eq!(k.runs().get(ctx.id())?.config_version.as_deref(), Some(v1.as_str()));
    ctx.apply_latest_config();
    assert_ne!(ctx.config().version, v1, "'nu toepassen' pakt de nieuwe versie");
    ctx.start()?;
    ctx.complete(serde_json::json!(null))?;
    Ok(())
}

#[tokio::test]
async fn gpu_slot_is_exclusive_between_runs() -> Result<()> {
    let d = tempfile::tempdir()?;
    let k = kernel(d.path()).await?;
    let a = k.start_run(RunSpec::new(RunKind::Ingest, Priority::P2))?;
    a.start()?;
    let permit = a.gpu(Priority::P2).await?;
    let b = k.start_run(RunSpec::new(RunKind::Chat, Priority::P0))?;
    b.start()?;
    let waiting = tokio::time::timeout(Duration::from_millis(50), b.gpu(Priority::P0)).await;
    assert!(waiting.is_err(), "tweede run moet wachten");
    drop(permit);
    let p2 = tokio::time::timeout(Duration::from_millis(500), b.gpu(Priority::P0)).await;
    assert!(p2.is_ok());
    Ok(())
}
