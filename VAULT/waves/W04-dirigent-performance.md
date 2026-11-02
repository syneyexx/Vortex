---
wave: W04
status: bezig  # code af 2026-11-02; Windows-verificatie W04.02/03
modules: [orkestrator-performance, runtime-core]
---
# W04 — Dirigent en performance
Zie [[modules/orkestrator-performance]]. Afgesplitst uit W01 (besluit 2026-11-02): eerst werkt chat end-to-end (W00–W03), daarna komt de volledige Dirigent — vóór geheugen, ingest en achtergrondwerk, want vanaf dan draait er meer dan één soort werk tegelijk.

- [x] `W04.01` Dirigent volgens [[modules/orkestrator-performance]]: taakaanmelding met middelenvraag, P0–P3, admission control per middel, preemption op veilige punten, priority inheritance, model-affiniteit, pipelining, bulkheads, backpressure, drukmeters, idle-detectie — **af:** `vortex-core::dirigent` (12 tests) + `sampler` (drukmeters, idle, externe GPU-last) + pauzeren/hervatten (`hold`); RunContext::gpu loopt via de Dirigent. Idle-tijd op Windows (GetLastInputInfo) typecheckt — gedrag **verifiëren op de PC**
- [~] `W04.02` Twee tokio-runtimes (CCD0 interactief / CCD1 achtergrond) + CPU-affiniteit en Windows-prioriteit per proces — twee runtimes met core-pinning + CCD-indeling (Windows- en Linux-nummering) + `place_process` af en getest op Linux; Windows-affiniteit/prioriteit typecheckt — **verifiëren op de PC**
- [~] `W04.03` RAM-budget per modus + harde caps (Job Objects) + nette herstart na checkpoint — RAM-budget per modus + RSS-bewaking met event/callback af en getest; harde cap via Job Object (Windows) typecheckt — **verifiëren op de PC**
- [x] `W04.04` HDD: één I/O-wachtrij, gesorteerd, sequentieel
- [x] `W04.05` Circuit breakers + bulkheads
- [x] `W04.06` Stall/livelock- en retry-amplification-detectie (signalen + events)
- [x] `W04.07` Single-flight + negative caching utilities

## Gate
Chat (P0) onderbreekt een lopende achtergrondtaak op een veilig punt en krijgt de GPU; onder belasting nooit OOM (admission control); drukmeters en wachtrijen live zichtbaar in statusbalk en Lopend werk; geen proces boven zijn RAM-cap.
