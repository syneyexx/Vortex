//! Crash-herstel en crash-rapport: wat voltooid is, wat liep, wat echt gestopt is, welke
//! externe actie mogelijk al gebeurde, wat veilig hervat kan en waar ingrijpen nodig is.
// VX:W01.17

use std::path::{Path, PathBuf};

use serde::Serialize;
use vortex_types::{Event, EventKind, RunId, RunState, StopStatus, TraceId};

use crate::checkpoint::{Checkpoints, EffectRecord};
use crate::error::Result;
use crate::events::EventLog;
use crate::resources::{HeldResource, ResourceRegistry};
use crate::runs::{RunRecord, RunStore};

/// Korte verwijzing naar een run in het rapport.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRef {
    pub id: RunId,
    pub kind: String,
    pub state_before: RunState,
    pub state_after: RunState,
    pub checkpoint_seq: Option<i64>,
    pub note: String,
}

/// Het crash-rapport (ook bij een nette start, dan is `crashed = false`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct CrashReport {
    /// Vorige keer niet netjes afgesloten.
    pub crashed: bool,
    /// Runs die al voltooid waren (laatste 20), ter referentie.
    pub completed_recently: Vec<RunId>,
    /// Liep tijdens de crash.
    pub was_running: Vec<RunRef>,
    /// Stop was aangevraagd en is nu definitief gestopt.
    pub stopped: Vec<RunRef>,
    /// Effecten met status 'intent': extern mogelijk al uitgevoerd.
    pub possibly_external: Vec<EffectRecord>,
    /// Veilig teruggezet in de wachtrij (hervat vanaf laatste checkpoint).
    pub resumable: Vec<RunRef>,
    /// Stonden nog in de wachtrij; ongewijzigd.
    pub still_queued: Vec<RunId>,
    /// Afgesloten als mislukt.
    pub failed: Vec<RunRef>,
    /// Resources die nog geregistreerd stonden (processen, slots) — nu vrijgegeven.
    pub stale_resources: Vec<HeldResource>,
    /// Waar de gebruiker naar moet kijken.
    pub needs_intervention: Vec<String>,
}

impl CrashReport {
    /// Leesbare samenvatting (Nederlands) voor GUI/CLI.
    pub fn summary(&self) -> String {
        if !self.crashed && self.was_running.is_empty() && self.stopped.is_empty() {
            return "Nette start: niets te herstellen.".into();
        }
        let mut s = String::new();
        s.push_str(if self.crashed { "Vortex is niet netjes afgesloten.\n" } else { "Herstel uitgevoerd.\n" });
        s.push_str(&format!(
            "- liep: {} · hervatbaar: {} · gestopt: {} · mislukt: {} · in wachtrij: {}\n",
            self.was_running.len(),
            self.resumable.len(),
            self.stopped.len(),
            self.failed.len(),
            self.still_queued.len()
        ));
        if !self.possibly_external.is_empty() {
            s.push_str(&format!("- {} externe actie(s) mogelijk al uitgevoerd\n", self.possibly_external.len()));
        }
        if !self.stale_resources.is_empty() {
            s.push_str(&format!("- {} achtergebleven resource(s) vrijgegeven\n", self.stale_resources.len()));
        }
        for n in &self.needs_intervention {
            s.push_str(&format!("- INGRIJPEN: {n}\n"));
        }
        s
    }
}

/// Markeerbestand: bestaat zolang Vortex draait. Bij opstarten aanwezig = vorige keer gecrasht.
#[derive(Debug)]
pub struct RunningMarker {
    path: PathBuf,
}

impl RunningMarker {
    /// Plaats de marker; geeft terug of er al een stond (= crash).
    pub fn place(data_dir: &Path) -> Result<(Self, bool)> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("vortex.running");
        let crashed = path.exists();
        std::fs::write(&path, std::process::id().to_string())?;
        Ok((Self { path }, crashed))
    }
    /// Nette afsluiting: marker weg.
    pub fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Voer herstel uit. Moet draaien vóór er nieuw werk start.
pub fn recover(
    log: &EventLog,
    runs: &RunStore,
    ckp: &Checkpoints,
    res: &ResourceRegistry,
    crashed: bool,
) -> Result<CrashReport> {
    let mut rep = CrashReport { crashed, ..Default::default() };
    rep.completed_recently =
        runs.recent(20)?.into_iter().filter(|r| r.state == RunState::Completed).map(|r| r.id).collect();

    // Achtergebleven resources: het proces dat ze hield bestaat niet meer.
    rep.stale_resources = res.held()?;
    for r in &rep.stale_resources {
        if r.kind == "process" {
            rep.needs_intervention.push(format!(
                "proces '{}' (pid {}) hoorde bij een gecrashte run; controleer of het nog draait",
                r.name,
                r.detail.get("pid").cloned().unwrap_or_default()
            ));
        }
        res.release(r.id)?;
    }

    for run in runs.active()? {
        handle_run(&run, runs, ckp, &mut rep)?;
    }

    log.append(Event::new(
        EventKind::SystemRecovered,
        TraceId::new(),
        serde_json::json!({
            "crashed": crashed,
            "resumable": rep.resumable.len(),
            "failed": rep.failed.len(),
            "stopped": rep.stopped.len(),
            "possibly_external": rep.possibly_external.len(),
            "needs_intervention": rep.needs_intervention,
        }),
    ))?;
    Ok(rep)
}

fn handle_run(run: &RunRecord, runs: &RunStore, ckp: &Checkpoints, rep: &mut CrashReport) -> Result<()> {
    let latest = ckp.latest(run.id)?.map(|c| c.seq);
    let mk = |after: RunState, note: &str| RunRef {
        id: run.id,
        kind: run.kind.as_str().to_string(),
        state_before: run.state,
        state_after: after,
        checkpoint_seq: latest,
        note: note.to_string(),
    };
    match run.state {
        RunState::Queued => rep.still_queued.push(run.id),
        RunState::Cancelling => {
            runs.set_stop_status(run.id, StopStatus::Stopped)?;
            runs.recover_transition(
                run.id,
                RunState::Cancelled,
                "stop aangevraagd vóór crash; proces bestaat niet meer",
            )?;
            runs.set_stop_status(run.id, StopStatus::CleanedUp)?;
            rep.stopped.push(mk(RunState::Cancelled, "gestopt"));
        }
        RunState::Starting | RunState::Running | RunState::Waiting => {
            let pending = ckp.effects(Some(run.id), Some("intent"))?;
            let external: Vec<_> = pending.iter().filter(|e| e.kind == "external").cloned().collect();
            rep.was_running.push(mk(run.state, "liep tijdens crash"));
            if !external.is_empty() {
                let r = runs.recover_transition(
                    run.id,
                    RunState::Failed,
                    "onderbroken door crash; externe actie mogelijk al uitgevoerd",
                )?;
                rep.needs_intervention.push(format!(
                    "run {} ({}): {} externe actie(s) mogelijk uitgevoerd: {}",
                    run.id,
                    run.kind.as_str(),
                    external.len(),
                    external.iter().map(|e| e.description.as_str()).collect::<Vec<_>>().join("; ")
                ));
                rep.failed.push(mk(r.state, "niet automatisch hervat: externe effecten onzeker"));
            } else if run.resumable {
                runs.recover_transition(run.id, RunState::Queued, "hervat na crash")?;
                rep.resumable.push(mk(
                    RunState::Queued,
                    if latest.is_some() { "hervat vanaf checkpoint" } else { "opnieuw vanaf begin" },
                ));
            } else {
                runs.recover_transition(run.id, RunState::Failed, "onderbroken door crash (niet hervatbaar)")?;
                rep.failed.push(mk(RunState::Failed, "niet hervatbaar"));
            }
            rep.possibly_external.extend(external);
        }
        _ => {}
    }
    Ok(())
}
