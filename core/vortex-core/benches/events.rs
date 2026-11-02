//! Hete paden van de kern (W06.13): event toevoegen (outbox) en lezen.
//! Draaien: `cargo bench -p vortex-core`.
use std::sync::Arc;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use vortex_core::{Db, EventLog, db::CORE_MIGRATIONS};
use vortex_types::{Event, EventKind, TraceId};

fn bench(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let log =
        EventLog::new(Arc::new(Db::open(dir.path().join("b.db"), CORE_MIGRATIONS).unwrap_or_else(|e| panic!("{e}"))));
    let t = TraceId::new();
    c.bench_function("event_append_wal", |b| {
        b.iter(|| black_box(log.append(Event::new(EventKind::MessageDelta, t, serde_json::json!({"text": "abc"})))))
    });
    c.bench_function("event_read_100", |b| b.iter(|| black_box(log.read_from(0, 100))));
}

criterion_group! { name = benches; config = Criterion::default().sample_size(20); targets = bench }
criterion_main!(benches);
