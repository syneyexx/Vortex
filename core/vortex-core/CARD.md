# CARD — `vortex-core`

**Doel:** runtime kern — eigenaar van runs, statussen, stoppen, budgetten, checkpoints, resources, config, secrets, processen en de GPU-slot-scheduler.
**VAULT-module:** VAULT/modules/runtime-core.md · **Waves:** W01, W04

## Publieke API
| Item | Doet |
| --- | --- |
| `Kernel::open(KernelOptions) -> (Kernel, CrashReport)` | opent DB, doet crash-herstel, laadt config, start config-watcher |
| `Kernel::start_run(RunSpec) -> RunContext` | nieuwe run (queued) met token, budget, trace, config-versie |
| `Kernel::resume_run(id)` | hervat queued run na crash (+ laatste checkpoint) |
| `Kernel::request_stop(id)` | stopknop: hele keten (kinderen, processen) |
| `Kernel::shutdown(grace)` | nette afsluiting (marker weg → volgende start geen crash) |
| `RunContext::{start, wait, resume, step, charge_tokens, checkpoint, effect, gpu, spawn_process, child, complete, fail, finish_cancelled}` | alles binnen één run; drop zonder afronding = veilig afgesloten |
| `EventLog::{append, transact, subscribe, read_from, for_run, log_decision, log_outcome, mark_processed}` | event log + outbox |
| `Dispatcher` + `EventConsumer` | at-least-once levering met opgeslagen positie |
| `RunStore` | state machine (`runs::allowed`), optimistic concurrency, snapshot-reads |
| `BudgetTracker`, `Deadline` | budgetten + deadline-propagatie naar kinderen |
| `TaskScope`, `DetachedTasks` | structured concurrency |
| `bounded()` | begrensde wachtrij met backpressure |
| `ResourceRegistry` / `ResourceGuard` | resource ownership (persistent) |
| `ConfigManager` / `ConfigSnapshot` | versies, hot-reload, geschiedenis |
| `SecretBroker` / `Secret` | OS-keystore, nooit in logs |
| `Supervisor` / `ProcessSpec` | procesgroep, outputlimiet, boom stoppen |
| `SlotScheduler` | eenvoudige slot-wachtrij (bouwsteen) |
| `Dirigent` / `TaskSpec` / `Demand` / `Lease` | **alle taken**: P0–P3, admission, preemption (`should_yield`/`yield_now`), boost, affiniteit, bulkheads, condities, `hold` (pauze), kostenmodel; `RunContext::{gpu, admit}` |
| `sampler::spawn` | drukmeters + idle + externe GPU-last → `Dirigent::set_conditions` |
| `runtimes::{Runtimes, CpuLayout, place_process}` | CCD0 interactief / CCD1 achtergrond, core-pinning, procesplaatsing |
| `platform` | Windows: Job Object-cap, affiniteit, prioriteit, idle-tijd (unsafe, alleen hier) |
| `memwatch::{RamBudget, watch_process}` | RAM-budget per modus + RSS-bewaking |
| `HddQueue` | één I/O-thread, elevator-sortering |
| `CircuitBreaker`, `Bulkhead` | resilience |
| `StallDetector`, `RetryBudget` | livelock / retry-amplification → events |
| `SingleFlight`, `NegativeCache` | dedupe + korte TTL voor fouten |
| `telemetry::{init, TraceCtx}` | tracing met trace/span/run-ID's |

## Afhankelijk van
`vortex-types` · SQLite (rusqlite, bundled) · tokio · notify · keyring

## Events (schrijft)
`run.*`, `resource.ownership.*`, `process.supervisor.*`, `scheduler.slot.*`, `config.version.reloaded`, `decision.*`, `system.*` — register: `vortex-types/src/event.rs`.

## Invarianten
- Event log is append-only (DB-triggers). Status + event altijd in één transactie.
- `completed` alleen met opgeslagen uitvoer; `failed` alleen met fout; `cancelled` pas als stopstatus ≥ `stopped`.
- Stopstatus gaat alleen vooruit: requested → stopped → cleaned_up.
- Elke resource heeft een eigenaar; vrijgeven bij drop/panic; na crash in het rapport.
- Lopende run houdt zijn config-versie tot `apply_latest_config()`.

## Ingangen
| Bestand | Waarom |
| --- | --- |
| `src/kernel.rs` | begin hier: Kernel + RunContext |
| `src/dirigent.rs` | planning van alle taken |
| `src/runs.rs` | overgangstabel + voorwaarden |
| `src/events.rs` | event log, outbox, dispatcher |
| `src/recovery.rs` | crash-rapport |
| `migrations/0001_core.sql` | schema |

## Tests
`cargo test -p vortex-core` — unit (81) + `tests/kernel.rs` (stopketen < 500 ms, budget, config-versie, GPU-slot) + `tests/crash.rs` (W01-gate: kill -9 → exact herstel).

## One-brain-check
- [x] event log · [ ] workspace (W07) · [ ] geheugen (W05) · [x] metacog/neural (feature-logging `DecisionRecord`) · [ ] GUI (W03)
