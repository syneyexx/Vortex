---
type: module
crate: vortex-core
waves: [W01]
updated: 2026-10-31
---
# Runtime core

Doel: na crash, afsluiten of hangende tool precies weten **wat voltooid is, wat draait, wat echt gestopt is, welke externe actie mogelijk al gebeurde, wat veilig hervat kan, waar ingrijpen nodig is**.

## Onderdelen
- **Control plane vs data plane** — kern bepaalt wat draait; workers voeren uit. Vastlopende worker neemt bediening/stopknop niet mee.
- **Run state machine** — queued, starting, running, waiting, cancelling, completed, failed, cancelled. Overgangen met voorwaarden ("completed" pas na opslag definitieve uitvoer).
- **Structured concurrency** — subtaken in scope van ouder; bij einde opruimen; losgekoppelde achtergrondtaken krijgen eigen eigenaar.
- **Cancellation propagation** — UI → core → agent → tool → subprocess → inferenceworker. Statussen: stop aangevraagd / gestopt / opgeruimd.
- **Deadline propagation** — één eindtijd, subtaken erven resterend budget.
- **Budgetten** — max stappen, tokens, tijd per taak.
- **Bounded queues + backpressure.**
- **Resource ownership** — GPU-reservering, proces, handle, stream, lock hebben eigenaar; vrijgeven bij einde/crash.
- **Priority inheritance** — achtergrondtaak waarop chat wacht krijgt tijdelijk hoge prioriteit.
- **Circuit breakers** + **bulkheads** (aparte limieten: chat, parsing, embeddings, tools).
- **Event log** — append-only, outbox (event + status in één transactie), dispatcher, ontvangers idempotent.
- **Checkpoints + hervatten.**
- **Optimistic concurrency** (versies), **snapshot-consistente reads**.
- **Config** — versies, hot-reload uit `config/`; lopende run houdt eigen versies.
- **Scheduler** — één taak tegelijk op hoofd-GPU, actieve chat voorrang, admission control (gewichten + KV + buffers passen?), idle-detectie (koppeling [[modules/autonomie]]).
- **Secrets broker** — sleutels nooit in context/geheugen (techniek, geen rem).
- **Subprocess-supervisie** — procesgroepen, outputlimieten.
- **Stall/livelock-detectie** — zelfde tool herhalen, zelfde fout, plan herschrijven zonder uitvoering, geen nieuw resultaat.
- **Retry-amplification-detectie.**
- **Effectanalyse** — stap = lezen / lokaal wijzigen / extern → bepaalt parallel (gegevens uit toolcontracten, [[modules/tools-skills]]).
- **Single-flight**, **negative caching** (korte TTL), **dead-task elimination**, **common subexpression elimination**, **incremental recomputation** (W16).

## Verbindingen
Event log → [[modules/observability-eval]], [[modules/training]], [[modules/geheugen]]. Scheduler ↔ [[modules/inference-modelbeheer]]. Budget ↔ [[modules/cognitie]].
