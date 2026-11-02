//! Event log (append-only) + outbox + dispatcher met idempotente ontvangers.
//!
//! * **Outbox**: statuswijziging en events in één SQLite-transactie ([`EventLog::transact`]).
//!   Pas ná de commit worden events live uitgezonden (broadcast) en wordt de dispatcher gewekt.
//! * **Dispatcher**: elke ontvanger heeft een eigen positie (`consumer_offsets`); levering is
//!   at-least-once in volgorde. Ontvangers zijn idempotent via `event.id` of [`EventLog::mark_processed`].
// VX:W01.02

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use tokio::sync::{Notify, broadcast};
use tokio_util::sync::CancellationToken;
use vortex_types::{DecisionRecord, Event, EventId, EventKind, Provenance, RunId, TraceId};

use crate::db::Db;
#[cfg(test)]
use crate::error::CoreError;
use crate::error::Result;

/// Capaciteit van de live-stream (begrensd; trage luisteraars missen events en lezen bij via seq).
pub const LIVE_CAPACITY: usize = 4096;

/// Events die in een transactie zijn klaargezet.
#[derive(Debug, Default)]
pub struct Outbox {
    events: Vec<Event>,
}

impl Outbox {
    /// Event opslaan binnen de lopende transactie (krijgt direct een volgnummer).
    pub fn emit(&mut self, tx: &Transaction<'_>, mut event: Event) -> Result<i64> {
        event.seq = insert_event(tx, &event)?;
        let seq = event.seq;
        self.events.push(event);
        Ok(seq)
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

fn insert_event(conn: &Connection, e: &Event) -> Result<i64> {
    conn.execute(
        "INSERT INTO events(id, kind, schema_version, ts, run_id, trace_id, span_id, provenance, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            e.id.to_string(),
            e.kind.as_str(),
            e.schema_version,
            e.ts.to_rfc3339(),
            e.run_id.map(|r| r.to_string()),
            e.trace_id.to_string(),
            e.span_id.map(|s| s.to_string()),
            serde_json::to_string(&e.provenance)?,
            serde_json::to_string(&e.payload)?,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    fn conv<T, E: std::fmt::Display>(i: usize, v: std::result::Result<T, E>) -> rusqlite::Result<T> {
        v.map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.to_string().into()))
    }
    let kind: String = r.get(2)?;
    let ts: String = r.get(4)?;
    let run_id: Option<String> = r.get(5)?;
    let span: Option<String> = r.get(7)?;
    let prov: String = r.get(8)?;
    let payload: String = r.get(9)?;
    Ok(Event {
        seq: r.get(0)?,
        id: conv(1, r.get::<_, String>(1)?.parse())?,
        kind: conv(2, EventKind::parse(&kind).ok_or(format!("onbekend event {kind}")))?,
        schema_version: r.get(3)?,
        ts: conv(4, chrono::DateTime::parse_from_rfc3339(&ts))?.with_timezone(&chrono::Utc),
        run_id: run_id.map(|s| conv(5, s.parse::<RunId>())).transpose()?,
        trace_id: conv(6, r.get::<_, String>(6)?.parse::<TraceId>())?,
        span_id: span.map(|s| conv(7, s.parse())).transpose()?,
        provenance: conv(8, serde_json::from_str::<Provenance>(&prov))?,
        payload: conv(9, serde_json::from_str(&payload))?,
    })
}

const SELECT: &str =
    "SELECT seq, id, kind, schema_version, ts, run_id, trace_id, span_id, provenance, payload FROM events";

/// Het event log.
#[derive(Debug, Clone)]
pub struct EventLog {
    db: Arc<Db>,
    live: broadcast::Sender<Event>,
    wake: Arc<Notify>,
}

impl EventLog {
    pub fn new(db: Arc<Db>) -> Self {
        let (live, _) = broadcast::channel(LIVE_CAPACITY);
        Self { db, live, wake: Arc::new(Notify::new()) }
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    /// Outbox-transactie: `f` mag de database wijzigen en events klaarzetten; alles of niets.
    pub fn transact<T>(&self, f: impl FnOnce(&Transaction<'_>, &mut Outbox) -> Result<T>) -> Result<T> {
        let mut outbox = Outbox::default();
        let out = self.db.write(|tx| f(tx, &mut outbox))?;
        self.publish(outbox.events);
        Ok(out)
    }

    /// Eén event opslaan (eigen transactie). Geeft het volgnummer terug.
    pub fn append(&self, event: Event) -> Result<i64> {
        self.transact(|tx, out| out.emit(tx, event))
    }

    /// Beslissing loggen met features (trainingsdata). Geeft het decision-ID terug.
    pub fn log_decision(&self, trace: TraceId, run: Option<RunId>, record: DecisionRecord) -> Result<EventId> {
        let id = record.decision_id;
        let mut e = Event::new(EventKind::DecisionMade, trace, serde_json::to_value(&record)?);
        e.id = id;
        e.run_id = run;
        self.append(e)?;
        Ok(id)
    }

    /// Uitkomst van een eerdere beslissing vastleggen.
    pub fn log_outcome(&self, trace: TraceId, decision_id: EventId, outcome: serde_json::Value) -> Result<i64> {
        self.append(Event::new(
            EventKind::DecisionOutcome,
            trace,
            serde_json::json!({ "decision_id": decision_id, "outcome": outcome }),
        ))
    }

    fn publish(&self, events: Vec<Event>) {
        if events.is_empty() {
            return;
        }
        for e in events {
            // Geen luisteraars is geen fout.
            let _ = self.live.send(e);
        }
        self.wake.notify_waiters();
        self.wake.notify_one();
    }

    /// Live meeluisteren (GUI). Gemiste events (lag) haal je op met [`EventLog::read_from`].
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.live.subscribe()
    }

    /// Events met seq > `after`, maximaal `limit`.
    pub fn read_from(&self, after: i64, limit: usize) -> Result<Vec<Event>> {
        self.db.read(|c| {
            let mut st = c.prepare(&format!("{SELECT} WHERE seq > ?1 ORDER BY seq LIMIT ?2"))?;
            let rows = st.query_map(params![after, limit as i64], row_to_event)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Alle events van één run, in volgorde.
    pub fn for_run(&self, run: RunId) -> Result<Vec<Event>> {
        self.db.read(|c| {
            let mut st = c.prepare(&format!("{SELECT} WHERE run_id = ?1 ORDER BY seq"))?;
            let rows = st.query_map(params![run.to_string()], row_to_event)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Hoogste volgnummer.
    pub fn last_seq(&self) -> Result<i64> {
        self.db.read(|c| Ok(c.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?))
    }

    /// Idempotentie-hulp: `true` als `key` voor `consumer` nog niet verwerkt was (en markeert hem nu).
    pub fn mark_processed(&self, consumer: &str, key: &str) -> Result<bool> {
        self.db.write(|tx| {
            let n = tx.execute(
                "INSERT OR IGNORE INTO consumer_processed(consumer, key, ts) VALUES (?1, ?2, ?3)",
                params![consumer, key, vortex_types::now().to_rfc3339()],
            )?;
            Ok(n == 1)
        })
    }

    fn offset(&self, consumer: &str) -> Result<i64> {
        self.db.read(|c| {
            Ok(c.query_row("SELECT last_seq FROM consumer_offsets WHERE consumer = ?1", [consumer], |r| r.get(0))
                .optional()?
                .unwrap_or(0))
        })
    }

    fn set_offset(&self, consumer: &str, seq: i64) -> Result<()> {
        self.db.write(|tx| {
            tx.execute(
                "INSERT INTO consumer_offsets(consumer, last_seq) VALUES (?1, ?2)
                 ON CONFLICT(consumer) DO UPDATE SET last_seq = excluded.last_seq",
                params![consumer, seq],
            )?;
            Ok(())
        })
    }

    fn wake(&self) -> Arc<Notify> {
        self.wake.clone()
    }
}

/// Ontvanger van events. Moet idempotent zijn: na een crash kan hetzelfde event opnieuw komen.
pub trait EventConsumer: Send + Sync {
    /// Unieke, stabiele naam (positie wordt hieronder opgeslagen).
    fn name(&self) -> &str;
    /// Verwerk één event. Fout = later opnieuw proberen vanaf dit event.
    fn handle(&self, event: &Event) -> std::result::Result<(), String>;
}

/// Levert events uit het log aan geregistreerde ontvangers.
#[derive(Clone)]
pub struct Dispatcher {
    log: EventLog,
    consumers: Arc<Mutex<Vec<Arc<dyn EventConsumer>>>>,
    batch: usize,
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher").field("consumers", &self.consumers.lock().len()).finish()
    }
}

impl Dispatcher {
    pub fn new(log: EventLog) -> Self {
        Self { log, consumers: Arc::new(Mutex::new(Vec::new())), batch: 256 }
    }

    pub fn register(&self, c: Arc<dyn EventConsumer>) {
        self.consumers.lock().push(c);
    }

    /// Verwerk alles wat klaarstaat. Geeft aantal afgeleverde events terug.
    pub fn run_once(&self) -> Result<usize> {
        let consumers: Vec<_> = self.consumers.lock().clone();
        let mut delivered = 0;
        for c in consumers {
            let mut offset = self.log.offset(c.name())?;
            loop {
                let events = self.log.read_from(offset, self.batch)?;
                if events.is_empty() {
                    break;
                }
                let mut failed = false;
                for e in &events {
                    match c.handle(e) {
                        Ok(()) => {
                            offset = e.seq;
                            delivered += 1;
                        }
                        Err(err) => {
                            tracing::warn!(consumer = c.name(), seq = e.seq, error = %err, "ontvanger faalde; later opnieuw");
                            failed = true;
                            break;
                        }
                    }
                }
                self.log.set_offset(c.name(), offset)?;
                if failed || events.len() < self.batch {
                    break;
                }
            }
        }
        Ok(delivered)
    }

    /// Draai op de achtergrond tot `stop`. Wakker bij nieuwe events, anders elke `poll`.
    pub fn spawn(self, stop: CancellationToken, poll: Duration) -> tokio::task::JoinHandle<()> {
        let wake = self.log.wake();
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.run_once() {
                    tracing::error!(error = %e, "dispatcher-fout");
                }
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = wake.notified() => {},
                    _ = tokio::time::sleep(poll) => {},
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CORE_MIGRATIONS;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn log() -> Result<EventLog> {
        Ok(EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS)?)))
    }

    #[test]
    fn append_assigns_increasing_seq_and_reads_back() -> Result<()> {
        let log = log()?;
        let t = TraceId::new();
        let a = log.append(Event::new(EventKind::SystemStarted, t, serde_json::json!({})))?;
        let b = log.append(Event::new(EventKind::SystemStarted, t, serde_json::json!({"x": 1})))?;
        assert!(b > a);
        let all = log.read_from(0, 10)?;
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].payload["x"], 1);
        Ok(())
    }

    #[test]
    fn log_is_append_only() -> Result<()> {
        let log = log()?;
        log.append(Event::new(EventKind::SystemStarted, TraceId::new(), serde_json::json!({})))?;
        let upd = log.db().write(|tx| Ok(tx.execute("UPDATE events SET kind = 'x'", [])?));
        assert!(upd.is_err());
        let del = log.db().write(|tx| Ok(tx.execute("DELETE FROM events", [])?));
        assert!(del.is_err());
        Ok(())
    }

    #[test]
    fn outbox_is_atomic_with_state_change() -> Result<()> {
        let log = log()?;
        let r: Result<()> = log.transact(|tx, out| {
            out.emit(tx, Event::new(EventKind::SystemStarted, TraceId::new(), serde_json::json!({})))?;
            Err(CoreError::Config("faalt na emit".into()))
        });
        assert!(r.is_err());
        assert_eq!(log.last_seq()?, 0, "event mag niet blijven staan als de transactie faalt");
        Ok(())
    }

    struct Counter {
        n: AtomicUsize,
        fail_at: Option<usize>,
    }
    impl EventConsumer for Counter {
        fn name(&self) -> &str {
            "counter"
        }
        fn handle(&self, _e: &Event) -> std::result::Result<(), String> {
            let n = self.n.load(Ordering::SeqCst);
            if Some(n) == self.fail_at {
                return Err("expres".into());
            }
            self.n.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn dispatcher_delivers_in_order_and_resumes_after_failure() -> Result<()> {
        let log = log()?;
        for _ in 0..5 {
            log.append(Event::new(EventKind::SystemStarted, TraceId::new(), serde_json::json!({})))?;
        }
        let d = Dispatcher::new(log.clone());
        let c = Arc::new(Counter { n: AtomicUsize::new(0), fail_at: Some(3) });
        d.register(c.clone());
        assert_eq!(d.run_once()?, 3);
        // tweede dispatcher (na "herstart") begint bij de opgeslagen positie, niet bij 0
        let d2 = Dispatcher::new(log.clone());
        let c2 = Arc::new(Counter { n: AtomicUsize::new(0), fail_at: None });
        d2.register(c2.clone());
        assert_eq!(d2.run_once()?, 2);
        Ok(())
    }

    #[test]
    fn mark_processed_is_idempotent() -> Result<()> {
        let log = log()?;
        assert!(log.mark_processed("c", "k1")?);
        assert!(!log.mark_processed("c", "k1")?);
        assert!(log.mark_processed("d", "k1")?);
        Ok(())
    }

    #[test]
    fn decision_and_outcome_are_logged_with_features() -> Result<()> {
        let log = log()?;
        let t = TraceId::new();
        let rec = DecisionRecord {
            decision_id: EventId::new(),
            decider: "router".into(),
            decider_version: "heuristiek-v1".into(),
            features: serde_json::json!({"len": 42}),
            choice: serde_json::json!("quick"),
            alternatives: vec![serde_json::json!("deep")],
            confidence: Some(0.7),
            outcome: None,
        };
        let id = log.log_decision(t, None, rec)?;
        log.log_outcome(t, id, serde_json::json!({"feedback": "goed"}))?;
        let ev = log.read_from(0, 10)?;
        assert_eq!(ev[0].kind, EventKind::DecisionMade);
        assert_eq!(ev[0].payload["features"]["len"], 42);
        assert_eq!(ev[1].payload["decision_id"], serde_json::json!(id));
        Ok(())
    }

    #[tokio::test]
    async fn live_subscribers_get_events_after_commit() -> Result<()> {
        let log = log()?;
        let mut rx = log.subscribe();
        log.append(Event::new(EventKind::SystemStarted, TraceId::new(), serde_json::json!({})))?;
        let e = rx.recv().await.map_err(|e| CoreError::Config(e.to_string()))?;
        assert_eq!(e.seq, 1);
        Ok(())
    }
}
