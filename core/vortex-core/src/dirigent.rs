//! De Dirigent: wijst álle taken toe aan de hardware (VAULT/modules/orkestrator-performance.md).
//!
//! * Taken melden zich aan met soort, prioriteit (P0–P3) en middelenvraag ([`Demand`]).
//! * **Admission control** per middel: past het niet → wachten, nooit OOM.
//! * **Preemption**: komt er iets belangrijkers, dan krijgen lagere, onderbreekbare taken een
//!   signaal en geven ze op een veilig punt hun middelen terug ([`Lease::yield_now`]).
//! * **Priority inheritance** ([`Dirigent::boost`]), **model-affiniteit** (taken voor het geladen
//!   model eerst), **bulkheads** (max per soort), **backpressure** (max wachtrij per soort).
//! * **Condities**: gebruiker actief / externe GPU-last (game) / geheugendruk bepalen of P2/P3
//!   mogen draaien; lopende taken die niet meer mogen krijgen een preempt-signaal.
//! * **Kostenmodel**: gemeten duur per soort (EWMA) — later een neurale layer.
//! * **Pipelining** volgt vanzelf: taken die verschillende middelen vragen (CPU, 1060, 5060 Ti)
//!   lopen tegelijk.
// VX:W04.01

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;
use vortex_types::{Event, EventKind, Priority, RunId, TraceId};

use crate::error::{CoreError, Result};
use crate::events::EventLog;

/// Middelenvraag (en capaciteit).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Demand {
    /// Slots op de hoofd-GPU (5060 Ti); capaciteit 1 = één taak tegelijk.
    pub main_gpu: u32,
    /// VRAM op de 1060 (MiB).
    pub aux_gpu_mib: u64,
    pub cores: u32,
    pub ram_mib: u64,
    pub ssd_io: u32,
    pub hdd_io: u32,
    pub net: u32,
}

impl Demand {
    pub fn main_gpu() -> Self {
        Self { main_gpu: 1, ..Default::default() }
    }
    fn fits(&self, free: &Demand) -> bool {
        self.main_gpu <= free.main_gpu
            && self.aux_gpu_mib <= free.aux_gpu_mib
            && self.cores <= free.cores
            && self.ram_mib <= free.ram_mib
            && self.ssd_io <= free.ssd_io
            && self.hdd_io <= free.hdd_io
            && self.net <= free.net
    }
    fn add(&self, o: &Demand) -> Demand {
        Demand {
            main_gpu: self.main_gpu + o.main_gpu,
            aux_gpu_mib: self.aux_gpu_mib + o.aux_gpu_mib,
            cores: self.cores + o.cores,
            ram_mib: self.ram_mib + o.ram_mib,
            ssd_io: self.ssd_io + o.ssd_io,
            hdd_io: self.hdd_io + o.hdd_io,
            net: self.net + o.net,
        }
    }
    fn sub(&self, o: &Demand) -> Demand {
        Demand {
            main_gpu: self.main_gpu.saturating_sub(o.main_gpu),
            aux_gpu_mib: self.aux_gpu_mib.saturating_sub(o.aux_gpu_mib),
            cores: self.cores.saturating_sub(o.cores),
            ram_mib: self.ram_mib.saturating_sub(o.ram_mib),
            ssd_io: self.ssd_io.saturating_sub(o.ssd_io),
            hdd_io: self.hdd_io.saturating_sub(o.hdd_io),
            net: self.net.saturating_sub(o.net),
        }
    }
    /// Delen elkaar minstens één middel?
    fn overlaps(&self, o: &Demand) -> bool {
        (self.main_gpu > 0 && o.main_gpu > 0)
            || (self.aux_gpu_mib > 0 && o.aux_gpu_mib > 0)
            || (self.cores > 0 && o.cores > 0)
            || (self.ram_mib > 0 && o.ram_mib > 0)
            || (self.ssd_io > 0 && o.ssd_io > 0)
            || (self.hdd_io > 0 && o.hdd_io > 0)
            || (self.net > 0 && o.net > 0)
    }
    fn uses_gpu(&self) -> bool {
        self.main_gpu > 0 || self.aux_gpu_mib > 0
    }
}

/// Een taak die zich aanmeldt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub name: String,
    /// Bulkhead-sleutel (bv. "chat", "ingest", "crawl", "training").
    pub kind: String,
    pub priority: Priority,
    pub demand: Demand,
    /// Model dat deze taak op de hoofd-GPU nodig heeft (model-affiniteit).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub run_id: Option<RunId>,
    /// Mag onderbroken worden op een veilig punt.
    #[serde(default = "yes")]
    pub preemptible: bool,
}

fn yes() -> bool {
    true
}

impl TaskSpec {
    pub fn new(name: &str, kind: &str, priority: Priority, demand: Demand) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            priority,
            demand,
            model: None,
            run_id: None,
            preemptible: priority != Priority::P0,
        }
    }
}

/// Omstandigheden die bepalen wat mag draaien.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Conditions {
    /// Gebruiker is lang genoeg niet actief geweest (P3 mag).
    pub user_idle: bool,
    /// Een ander programma (bv. een game) gebruikt de GPU → GPU-werk P2/P3 pauzeert.
    pub external_gpu_load: bool,
    /// 0 = normaal, 1 = hoog (geen nieuwe P3), 2 = kritiek (geen nieuwe P2/P3, lopende P3 pauzeren).
    pub pressure: u8,
}

/// Limieten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirigentConfig {
    pub capacity: Demand,
    /// Max tegelijk per soort (bulkheads). Ontbreekt = onbeperkt.
    #[serde(default)]
    pub bulkheads: HashMap<String, usize>,
    /// Max wachtenden per soort (backpressure).
    #[serde(default = "default_queue")]
    pub max_queue_per_kind: usize,
}

fn default_queue() -> usize {
    256
}

impl Default for DirigentConfig {
    fn default() -> Self {
        Self {
            // 5950X: 16 cores; 32 GB RAM; 1060 6 GB (≈5,5 GB bruikbaar); I/O-slots.
            capacity: Demand {
                main_gpu: 1,
                aux_gpu_mib: 5632,
                cores: 16,
                ram_mib: 24 * 1024,
                ssd_io: 8,
                hdd_io: 1,
                net: 16,
            },
            bulkheads: HashMap::from([
                ("parse".to_string(), 4),
                ("crawl".to_string(), 8),
                ("embed".to_string(), 2),
                ("tool".to_string(), 4),
                ("training".to_string(), 1),
            ]),
            max_queue_per_kind: default_queue(),
        }
    }
}

#[derive(Debug)]
struct Waiter {
    id: u64,
    spec: TaskSpec,
    eff: Priority,
    seq: u64,
    tx: oneshot::Sender<watch::Receiver<bool>>,
    queued_at: Instant,
}

#[derive(Debug)]
struct Running {
    spec: TaskSpec,
    eff: Priority,
    preempt: watch::Sender<bool>,
    started: Instant,
}

#[derive(Debug, Default)]
struct State {
    used: Demand,
    waiting: Vec<Waiter>,
    running: HashMap<u64, Running>,
    next: u64,
    loaded_model: Option<String>,
    cond: Conditions,
    /// EWMA van de duur per soort (ms).
    cost_ms: HashMap<String, f64>,
    /// Door de gebruiker gepauzeerde runs.
    held: std::collections::HashSet<RunId>,
}

/// Momentopname voor GUI/tracing (drukmeters + wachtrijen).
#[derive(Debug, Clone, Serialize)]
pub struct DirigentSnapshot {
    pub capacity: Demand,
    pub used: Demand,
    pub conditions: Conditions,
    pub loaded_model: Option<String>,
    pub running: Vec<(String, String, Priority)>,
    pub waiting: Vec<(String, String, Priority)>,
    pub cost_ms: HashMap<String, f64>,
}

/// De Dirigent (goedkoop te klonen).
#[derive(Debug, Clone)]
pub struct Dirigent {
    st: Arc<Mutex<State>>,
    cfg: Arc<DirigentConfig>,
    log: EventLog,
}

/// Aanmelding; wacht met [`Ticket::admitted`].
#[derive(Debug)]
pub struct Ticket {
    id: u64,
    rx: Option<oneshot::Receiver<watch::Receiver<bool>>>,
    d: Dirigent,
}

/// Toegekende middelen. Vrijgeven bij drop. Controleer [`Lease::should_yield`] op veilige punten.
#[derive(Debug)]
pub struct Lease {
    id: u64,
    d: Dirigent,
    preempt: watch::Receiver<bool>,
    released: bool,
}

type Event_ = (EventKind, serde_json::Value, Option<RunId>);

impl Dirigent {
    pub fn new(cfg: DirigentConfig, log: EventLog) -> Self {
        Self { st: Arc::new(Mutex::new(State::default())), cfg: Arc::new(cfg), log }
    }

    fn emit(&self, evs: Vec<Event_>) {
        for (k, p, run) in evs {
            let mut e = Event::new(k, TraceId::new(), p);
            e.run_id = run;
            let _ = self.log.append(e);
        }
    }

    /// Meld een taak aan. Vol (backpressure) = fout.
    pub fn submit(&self, spec: TaskSpec) -> Result<Ticket> {
        if !spec.demand.fits(&self.cfg.capacity) {
            return Err(CoreError::Config(format!("taak '{}' vraagt meer dan de machine heeft", spec.name)));
        }
        let (tx, rx) = oneshot::channel();
        let mut evs = Vec::new();
        let id = {
            let mut st = self.st.lock();
            let queued = st.waiting.iter().filter(|w| w.spec.kind == spec.kind).count();
            if queued >= self.cfg.max_queue_per_kind {
                return Err(CoreError::QueueFull(format!("dirigent:{}", spec.kind)));
            }
            st.next += 1;
            let id = st.next;
            evs.push((EventKind::TaskQueued, serde_json::json!({"task": id, "name": spec.name, "kind": spec.kind, "priority": spec.priority, "demand": spec.demand}), spec.run_id));
            st.waiting.push(Waiter { id, eff: spec.priority, seq: id, spec, tx, queued_at: Instant::now() });
            self.schedule(&mut st, &mut evs);
            id
        };
        self.emit(evs);
        Ok(Ticket { id, rx: Some(rx), d: self.clone() })
    }

    fn eligible(st: &State, spec: &TaskSpec) -> bool {
        if spec.run_id.is_some_and(|r| st.held.contains(&r)) {
            return false;
        }
        let cond = &st.cond;
        match spec.priority {
            Priority::P0 | Priority::P1 => true,
            Priority::P2 => cond.pressure < 2 && !(cond.external_gpu_load && spec.demand.uses_gpu()),
            Priority::P3 => cond.pressure == 0 && cond.user_idle && !cond.external_gpu_load,
        }
    }

    /// Kern van de planning. Wordt na elke wijziging aangeroepen (met lock).
    fn schedule(&self, st: &mut State, evs: &mut Vec<Event_>) {
        // 1. Lopende taken die niet meer mogen (condities) → preempt-signaal.
        for (id, r) in &st.running {
            if (r.spec.preemptible || r.spec.run_id.is_some_and(|x| st.held.contains(&x)))
                && !Self::eligible(st, &r.spec)
                && !*r.preempt.borrow()
            {
                let _ = r.preempt.send(true);
                evs.push((
                    EventKind::TaskPreemptRequested,
                    serde_json::json!({"task": id, "reason": "condities", "conditions": st.cond}),
                    r.spec.run_id,
                ));
            }
        }
        // 2. Wachtenden op volgorde: effectieve prioriteit, model-affiniteit, aanmeldvolgorde.
        let loaded = st.loaded_model.clone();
        st.waiting.sort_by_key(|w| {
            let affinity = match (&w.spec.model, &loaded) {
                (Some(m), Some(l)) if m == l => 0,
                (None, _) => 0,
                _ => 1,
            };
            (w.eff, affinity, w.seq)
        });
        let mut blocked: Vec<Demand> = Vec::new();
        let mut i = 0;
        while i < st.waiting.len() {
            let w = &st.waiting[i];
            let free = self.cfg.capacity.sub(&st.used);
            let kind_running = st.running.values().filter(|r| r.spec.kind == w.spec.kind).count();
            let bulk_ok = self.cfg.bulkheads.get(&w.spec.kind).is_none_or(|max| kind_running < *max);
            let elig = Self::eligible(st, &w.spec);
            // Geen "backfill" op middelen die een hoger-geprioriteerde wachtende nodig heeft.
            let conflicts = blocked.iter().any(|b| b.overlaps(&w.spec.demand));
            if elig && bulk_ok && !conflicts && w.spec.demand.fits(&free) {
                let w = st.waiting.remove(i);
                let (ptx, prx) = watch::channel(false);
                if w.tx.send(prx).is_err() {
                    continue; // aanmelder is weg (geannuleerd)
                }
                st.used = st.used.add(&w.spec.demand);
                evs.push((
                    EventKind::TaskAdmitted,
                    serde_json::json!({"task": w.id, "name": w.spec.name, "kind": w.spec.kind, "priority": w.eff,
                        "waited_ms": w.queued_at.elapsed().as_millis() as u64}),
                    w.spec.run_id,
                ));
                st.running.insert(w.id, Running { spec: w.spec, eff: w.eff, preempt: ptx, started: Instant::now() });
                continue;
            }
            if elig && bulk_ok && !w.spec.demand.fits(&free) {
                // Preemption: lagere, onderbreekbare taken die dezelfde middelen gebruiken.
                let need = w.spec.demand;
                let wid = w.id;
                let weff = w.eff;
                let mut victims: Vec<(u64, Priority)> = st
                    .running
                    .iter()
                    .filter(|(_, r)| r.spec.preemptible && r.eff > weff && r.spec.demand.overlaps(&need))
                    .map(|(id, r)| (*id, r.eff))
                    .collect();
                victims.sort_by_key(|(id, p)| (std::cmp::Reverse(*p), std::cmp::Reverse(*id)));
                let mut freed = free;
                for (vid, _) in victims {
                    if need.fits(&freed) {
                        break;
                    }
                    if let Some(r) = st.running.get(&vid) {
                        if !*r.preempt.borrow() {
                            let _ = r.preempt.send(true);
                            evs.push((
                                EventKind::TaskPreemptRequested,
                                serde_json::json!({"task": vid, "for": wid, "reason": "hogere prioriteit"}),
                                r.spec.run_id,
                            ));
                        }
                        freed = freed.add(&r.spec.demand);
                    }
                }
                blocked.push(need);
            }
            i += 1;
        }
    }

    fn release(&self, id: u64, yielded: bool) {
        let mut evs = Vec::new();
        {
            let mut st = self.st.lock();
            if let Some(r) = st.running.remove(&id) {
                st.used = st.used.sub(&r.spec.demand);
                let ms = r.started.elapsed().as_secs_f64() * 1000.0;
                if !yielded {
                    let e = st.cost_ms.entry(r.spec.kind.clone()).or_insert(ms);
                    *e = 0.8 * *e + 0.2 * ms;
                }
                evs.push((
                    if yielded { EventKind::TaskYielded } else { EventKind::TaskReleased },
                    serde_json::json!({"task": id, "name": r.spec.name, "kind": r.spec.kind, "ms": ms as u64}),
                    r.spec.run_id,
                ));
            }
            self.schedule(&mut st, &mut evs);
        }
        self.emit(evs);
    }

    fn cancel_waiter(&self, id: u64) {
        let mut evs = Vec::new();
        {
            let mut st = self.st.lock();
            st.waiting.retain(|w| w.id != id);
            self.schedule(&mut st, &mut evs);
        }
        self.emit(evs);
    }

    /// Priority inheritance: een taak (wachtend of lopend) van `run` krijgt tijdelijk `to`.
    pub fn boost(&self, run: RunId, to: Priority) -> usize {
        let mut evs = Vec::new();
        let n = {
            let mut st = self.st.lock();
            let mut n = 0;
            for w in st.waiting.iter_mut().filter(|w| w.spec.run_id == Some(run) && w.eff > to) {
                w.eff = to;
                n += 1;
            }
            for r in st.running.values_mut().filter(|r| r.spec.run_id == Some(run) && r.eff > to) {
                r.eff = to;
                n += 1;
            }
            if n > 0 {
                evs.push((EventKind::TaskBoosted, serde_json::json!({"to": to, "tasks": n}), Some(run)));
                self.schedule(&mut st, &mut evs);
            }
            n
        };
        self.emit(evs);
        n
    }

    /// Model dat nu op de hoofd-GPU staat (model-affiniteit).
    pub fn set_loaded_model(&self, model: Option<String>) {
        let mut evs = Vec::new();
        {
            let mut st = self.st.lock();
            st.loaded_model = model;
            self.schedule(&mut st, &mut evs);
        }
        self.emit(evs);
    }

    /// Nieuwe omstandigheden (idle-detectie, drukmeters, externe GPU-last).
    pub fn set_conditions(&self, c: Conditions) {
        let mut evs = Vec::new();
        {
            let mut st = self.st.lock();
            if st.cond == c {
                return;
            }
            st.cond = c;
            evs.push((EventKind::ConditionsChanged, serde_json::json!(c), None));
            self.schedule(&mut st, &mut evs);
        }
        self.emit(evs);
    }

    /// Pauzeer (vasthouden) of hervat alle taken van een run. Lopende taken krijgen een
    /// preempt-signaal en wijken op hun volgende veilige punt.
    pub fn hold(&self, run: RunId, hold: bool) {
        let mut evs = Vec::new();
        {
            let mut st = self.st.lock();
            if hold {
                st.held.insert(run);
            } else {
                st.held.remove(&run);
            }
            evs.push((EventKind::ConditionsChanged, serde_json::json!({"run": run, "held": hold}), Some(run)));
            self.schedule(&mut st, &mut evs);
        }
        self.emit(evs);
    }

    pub fn is_held(&self, run: RunId) -> bool {
        self.st.lock().held.contains(&run)
    }

    pub fn conditions(&self) -> Conditions {
        self.st.lock().cond
    }

    /// Geschatte duur van een soort taak (kostenmodel).
    pub fn estimate_ms(&self, kind: &str) -> Option<f64> {
        self.st.lock().cost_ms.get(kind).copied()
    }

    pub fn snapshot(&self) -> DirigentSnapshot {
        let st = self.st.lock();
        DirigentSnapshot {
            capacity: self.cfg.capacity,
            used: st.used,
            conditions: st.cond,
            loaded_model: st.loaded_model.clone(),
            running: st.running.values().map(|r| (r.spec.name.clone(), r.spec.kind.clone(), r.eff)).collect(),
            waiting: st.waiting.iter().map(|w| (w.spec.name.clone(), w.spec.kind.clone(), w.eff)).collect(),
            cost_ms: st.cost_ms.clone(),
        }
    }
}

impl Ticket {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Wacht tot de middelen er zijn (annuleerbaar).
    pub async fn admitted(mut self, cancel: &CancellationToken) -> Result<Lease> {
        let rx = self.rx.take().ok_or(CoreError::Cancelled)?;
        let r = tokio::select! {
            r = rx => r.map_err(|_| CoreError::Cancelled),
            _ = cancel.cancelled() => Err(CoreError::Cancelled),
        };
        match r {
            Ok(preempt) => Ok(Lease { id: self.id, d: self.d.clone(), preempt, released: false }),
            Err(e) => {
                // Misschien net toegekend tijdens annuleren: vrijgeven of uit de rij halen.
                self.d.cancel_waiter(self.id);
                self.d.release(self.id, true);
                Err(e)
            }
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if self.rx.is_some() {
            self.d.cancel_waiter(self.id);
        }
    }
}

impl Lease {
    /// Moet deze taak op het volgende veilige punt pauzeren?
    pub fn should_yield(&self) -> bool {
        *self.preempt.borrow()
    }

    /// Wacht tot een preempt-signaal komt (voor `select!` in lange taken).
    pub async fn preempted(&mut self) {
        while !*self.preempt.borrow() {
            if self.preempt.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }

    /// Op een veilig punt pauzeren: middelen teruggeven en opnieuw in de rij.
    pub async fn yield_now(mut self, spec: TaskSpec, cancel: &CancellationToken) -> Result<Lease> {
        self.released = true;
        self.d.release(self.id, true);
        let t = self.d.submit(spec)?;
        t.admitted(cancel).await
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if !self.released {
            self.d.release(self.id, false);
        }
    }
}

/// Wacht tot `d` tijd verstreken is of de lease moet wijken. Handig in lussen.
pub async fn sleep_or_yield(lease: &mut Lease, d: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(d) => false,
        _ = lease.preempted() => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{CORE_MIGRATIONS, Db};

    fn d() -> Result<Dirigent> {
        Ok(Dirigent::new(DirigentConfig::default(), EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?))))
    }
    fn gpu(name: &str, kind: &str, p: Priority) -> TaskSpec {
        TaskSpec::new(name, kind, p, Demand::main_gpu())
    }
    fn tok() -> CancellationToken {
        CancellationToken::new()
    }
    async fn admitted_now(t: Ticket) -> Option<Lease> {
        tokio::time::timeout(Duration::from_millis(30), t.admitted(&tok())).await.ok().and_then(|r| r.ok())
    }

    #[tokio::test]
    async fn admission_control_never_oversubscribes() -> Result<()> {
        let d = d()?;
        let a = admitted_now(d.submit(gpu("a", "ingest", Priority::P2))?).await;
        assert!(a.is_some());
        let b = d.submit(gpu("b", "ingest", Priority::P2))?;
        let b2 = tokio::spawn(async move { b.admitted(&tok()).await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!b2.is_finished(), "tweede GPU-taak moet wachten");
        assert_eq!(d.snapshot().used.main_gpu, 1);
        drop(a);
        assert!(tokio::time::timeout(Duration::from_millis(200), b2).await.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn chat_preempts_background_at_safe_point() -> Result<()> {
        let d = d()?;
        let spec = gpu("ingest", "ingest", Priority::P2);
        let lease = admitted_now(d.submit(spec.clone())?).await.ok_or(CoreError::Cancelled)?;
        assert!(!lease.should_yield());
        let chat = d.submit(gpu("chat", "chat", Priority::P0))?;
        let chat_h = tokio::spawn(async move { chat.admitted(&tok()).await.map(|_| Instant::now()) });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(lease.should_yield(), "achtergrond krijgt preempt-signaal");
        // Veilig punt: wijken en opnieuw aansluiten.
        let d2 = d.clone();
        let bg = tokio::spawn(async move { lease.yield_now(spec, &tok()).await.map(|_| Instant::now()) });
        let chat_at = chat_h.await.map_err(|e| CoreError::Config(e.to_string()))??;
        let bg_at = bg.await.map_err(|e| CoreError::Config(e.to_string()))??;
        assert!(chat_at <= bg_at, "chat eerst, daarna hervat de achtergrondtaak");
        assert_eq!(d2.snapshot().used.main_gpu, 0);
        Ok(())
    }

    #[tokio::test]
    async fn p0_is_not_preemptible() -> Result<()> {
        let d = d()?;
        let a = admitted_now(d.submit(gpu("chat1", "chat", Priority::P0))?).await.ok_or(CoreError::Cancelled)?;
        let _b = d.submit(gpu("chat2", "chat", Priority::P0))?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!a.should_yield());
        Ok(())
    }

    #[tokio::test]
    async fn idle_only_tasks_wait_for_idle_and_yield_on_game() -> Result<()> {
        let d = d()?;
        let t = d.submit(gpu("dreaming", "dream", Priority::P3))?;
        let h = tokio::spawn(async move { t.admitted(&tok()).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!h.is_finished(), "P3 wacht op idle");
        d.set_conditions(Conditions { user_idle: true, ..Default::default() });
        let lease = h.await.map_err(|e| CoreError::Config(e.to_string()))??;
        assert!(!lease.should_yield());
        d.set_conditions(Conditions { user_idle: true, external_gpu_load: true, pressure: 0 });
        assert!(lease.should_yield(), "game start → GPU vrijgeven");
        Ok(())
    }

    #[tokio::test]
    async fn pressure_blocks_low_priority() -> Result<()> {
        let d = d()?;
        d.set_conditions(Conditions { user_idle: true, external_gpu_load: false, pressure: 2 });
        let cpu = Demand { cores: 1, ..Default::default() };
        assert!(admitted_now(d.submit(TaskSpec::new("p2", "crawl", Priority::P2, cpu))?).await.is_none());
        assert!(admitted_now(d.submit(TaskSpec::new("p1", "crawl", Priority::P1, cpu))?).await.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn bulkheads_limit_per_kind_but_other_kinds_flow() -> Result<()> {
        let d = d()?;
        let cpu = Demand { cores: 1, ..Default::default() };
        let mut held = Vec::new();
        for i in 0..4 {
            held.push(
                admitted_now(d.submit(TaskSpec::new(&format!("p{i}"), "parse", Priority::P2, cpu))?)
                    .await
                    .ok_or(CoreError::Cancelled)?,
            );
        }
        assert!(
            admitted_now(d.submit(TaskSpec::new("p5", "parse", Priority::P2, cpu))?).await.is_none(),
            "bulkhead parse = 4"
        );
        assert!(
            admitted_now(d.submit(TaskSpec::new("c", "crawl", Priority::P2, cpu))?).await.is_some(),
            "andere soort loopt door (pipelining)"
        );
        Ok(())
    }

    #[tokio::test]
    async fn priority_inheritance_moves_task_forward() -> Result<()> {
        let d = d()?;
        let hold = admitted_now(d.submit(gpu("hold", "x", Priority::P1))?).await.ok_or(CoreError::Cancelled)?;
        let run = RunId::new();
        let mut s1 = gpu("needed-by-chat", "ingest", Priority::P2);
        s1.run_id = Some(run);
        let t_low = d.submit(gpu("other", "ingest", Priority::P2))?;
        let t_need = d.submit(s1)?;
        assert_eq!(d.boost(run, Priority::P0), 1);
        let h_need = tokio::spawn(async move { t_need.admitted(&tok()).await.map(|_| Instant::now()) });
        let h_low = tokio::spawn(async move {
            t_low.admitted(&tok()).await.map(|l| {
                drop(l);
                Instant::now()
            })
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(hold);
        let a = h_need.await.map_err(|e| CoreError::Config(e.to_string()))??;
        assert!(!h_low.is_finished() || a <= Instant::now());
        Ok(())
    }

    #[tokio::test]
    async fn model_affinity_prefers_loaded_model() -> Result<()> {
        let d = d()?;
        d.set_loaded_model(Some("code".into()));
        let hold = admitted_now(d.submit(gpu("hold", "x", Priority::P1))?).await.ok_or(CoreError::Cancelled)?;
        let mut a = gpu("needs-deep", "agent", Priority::P2);
        a.model = Some("deep".into());
        let mut b = gpu("needs-code", "agent", Priority::P2);
        b.model = Some("code".into());
        let ta = d.submit(a)?;
        let tb = d.submit(b)?;
        drop(hold);
        assert!(admitted_now(tb).await.is_some(), "taak voor geladen model eerst (geen wissel)");
        drop(ta);
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_waiter_leaves_no_trace() -> Result<()> {
        let d = d()?;
        let hold = admitted_now(d.submit(gpu("hold", "x", Priority::P1))?).await;
        let c = tok();
        let t = d.submit(gpu("w", "x", Priority::P2))?;
        let c2 = c.clone();
        let h = tokio::spawn(async move { t.admitted(&c2).await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(10)).await;
        c.cancel();
        assert!(h.await.map_err(|e| CoreError::Config(e.to_string()))?.is_err());
        drop(hold);
        assert!(d.snapshot().waiting.is_empty());
        assert_eq!(d.snapshot().used.main_gpu, 0);
        Ok(())
    }

    #[tokio::test]
    async fn backpressure_and_oversized_demand_are_errors() -> Result<()> {
        let cfg = DirigentConfig { max_queue_per_kind: 1, ..Default::default() };
        let d = Dirigent::new(cfg, EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?)));
        let _h = admitted_now(d.submit(gpu("a", "k", Priority::P2))?).await;
        let _w = d.submit(gpu("b", "k", Priority::P2))?;
        assert!(matches!(d.submit(gpu("c", "k", Priority::P2)), Err(CoreError::QueueFull(_))));
        assert!(
            d.submit(TaskSpec::new("x", "k", Priority::P2, Demand { ram_mib: 1 << 30, ..Default::default() })).is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn user_pause_and_resume() -> Result<()> {
        let d = d()?;
        let run = RunId::new();
        let mut spec = gpu("ingest", "ingest", Priority::P2);
        spec.run_id = Some(run);
        let lease = admitted_now(d.submit(spec.clone())?).await.ok_or(CoreError::Cancelled)?;
        d.hold(run, true);
        assert!(lease.should_yield());
        let h = tokio::spawn(async move { lease.yield_now(spec, &tok()).await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!h.is_finished(), "gepauzeerd blijft wachten");
        d.hold(run, false);
        assert!(tokio::time::timeout(Duration::from_millis(200), h).await.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn cost_model_learns_duration() -> Result<()> {
        let d = d()?;
        let l = admitted_now(d.submit(gpu("a", "embed", Priority::P2))?).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(l);
        assert!(d.estimate_ms("embed").unwrap_or(0.0) >= 15.0);
        Ok(())
    }
}
