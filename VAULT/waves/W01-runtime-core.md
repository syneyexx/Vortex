---
wave: W01
status: open
modules: [runtime-core, training]
---
# W01 — Runtime core
Zie [[modules/runtime-core]].

- [ ] `W01.01` SQLite (WAL) + migratiesysteem met schemaversie
- [ ] `W01.02` Event log: append-only tabel, event-typen (bericht, plan, toolcall, retrieval, debat, oordeel, feedback, model/promptversie, lifecycle), outbox + dispatcher, idempotente ontvangers
- [ ] `W01.03` Feature-logging-velden voor training (features, keuze, uitkomst) vanaf nu in elk beslis-event
- [ ] `W01.04` Run state machine met overgangsvoorwaarden + tests per overgang
- [ ] `W01.05` Structured concurrency (scopes, opruimen, losgekoppelde taken met eigenaar)
- [ ] `W01.06` Cancellation door hele keten met drie statussen
- [ ] `W01.07` Deadline propagation + budgetten (stappen, tokens, tijd)
- [ ] `W01.08` Bounded queues + backpressure
- [ ] `W01.09` Resource ownership registry (GPU-slot, proces, handle, lock)
- [ ] `W01.10` Dirigent volgens [[modules/orkestrator-performance]]: taakaanmelding met middelenvraag, P0–P3, admission control per middel, preemption op veilige punten, priority inheritance, model-affiniteit, pipelining, bulkheads, backpressure, drukmeters, idle-detectie
- [ ] `W01.11` Twee tokio-runtimes (CCD0 interactief / CCD1 achtergrond) + CPU-affiniteit en Windows-prioriteit per proces
- [ ] `W01.12` RAM-budget per modus + harde caps (Job Objects) + nette herstart na checkpoint
- [ ] `W01.13` HDD: één I/O-wachtrij, gesorteerd, sequentieel
- [ ] `W01.14` Allocator (mimalloc/jemalloc), begrensde kanalen/caches overal, tokio-console
- [ ] `W01.15` Circuit breakers + bulkheads
- [ ] `W01.16` Checkpoints + hervatten na crash (crash-test)
- [ ] `W01.17` Optimistic concurrency + snapshot-reads
- [ ] `W01.18` Config-systeem: versies, hot-reload, run houdt eigen versie
- [ ] `W01.19` Secrets broker (OS-keystore), nooit in context/log
- [ ] `W01.20` Subprocess-supervisie (procesgroepen, outputlimieten)
- [ ] `W01.21` Stall/livelock- en retry-amplification-detectie (signalen + events)
- [ ] `W01.22` Single-flight + negative caching utilities
- [ ] `W01.23` Tracing (trace/span-IDs) in alle bovenstaande
- [ ] `W01.24` Crash-rapport: wat voltooid / liep / gestopt / extern mogelijk uitgevoerd / hervatbaar / ingrijpen nodig

## Gate
Kill -9 midden in een run → herstart toont exact correcte status en hervat; alle tests groen.
