---
wave: W01
status: bezig  # Linux-gate groen 2026-11-02; Windows-verificatie W01.14/W01.15 open
modules: [runtime-core, training]
---
# W01 — Runtime kern
Zie [[modules/runtime-core]]. Alleen wat nodig is om betrouwbaar te draaien en te stoppen; de Dirigent en hardware-tuning zitten in [[waves/W04-dirigent-performance]] (besluit 2026-11-02).

- [x] `W01.01` SQLite (WAL) + migratiesysteem met schemaversie
- [x] `W01.02` Event log: append-only tabel, event-typen (bericht, plan, toolcall, retrieval, debat, oordeel, feedback, model/promptversie, lifecycle), outbox + dispatcher, idempotente ontvangers
- [x] `W01.03` Feature-logging-velden voor training (features, keuze, uitkomst) vanaf nu in elk beslis-event
- [x] `W01.04` Run state machine met overgangsvoorwaarden + tests per overgang
- [x] `W01.05` Structured concurrency (scopes, opruimen, losgekoppelde taken met eigenaar)
- [x] `W01.06` Cancellation door hele keten met drie statussen
- [x] `W01.07` Deadline propagation + budgetten (stappen, tokens, tijd)
- [x] `W01.08` Bounded queues + backpressure
- [x] `W01.09` Resource ownership registry (GPU-slot, proces, handle, lock)
- [x] `W01.10` Allocator (mimalloc/jemalloc), begrensde kanalen/caches overal, tokio-console
- [x] `W01.11` Checkpoints + hervatten na crash (crash-test)
- [x] `W01.12` Optimistic concurrency + snapshot-reads
- [x] `W01.13` Config-systeem: versies, hot-reload, run houdt eigen versie
- [~] `W01.14` Secrets broker (OS-keystore), nooit in context/log — broker + redactie af en getest (memory-backend); **OS-keystore (Windows Credential Manager) verifiëren op de PC**
- [~] `W01.15` Subprocess-supervisie (procesgroepen, outputlimieten) — af en getest op Linux (procesgroep, limiet, boom stoppen < 3 s, timeout); **Windows-pad (taskkill /T) verifiëren op de PC**
- [x] `W01.16` Tracing (trace/span-IDs) in alle bovenstaande
- [x] `W01.17` Crash-rapport: wat voltooid / liep / gestopt / extern mogelijk uitgevoerd / hervatbaar / ingrijpen nodig
- [x] `W01.18` Eenvoudige GPU-slot-scheduler: één taak tegelijk op de 5060 Ti, chat voorrang, wachtrij (volledige Dirigent in [[waves/W04-dirigent-performance]])

## Gate
Kill -9 midden in een run → herstart toont exact correcte status en hervat; alle tests groen.
