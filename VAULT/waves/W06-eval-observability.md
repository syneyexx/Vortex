---
wave: W06
status: bezig  # alles af behalve 🔒 grote tests (op commando)
modules: [observability-eval]
---
# W06 — Eval en observability
Zie [[modules/observability-eval]].

- [x] `W06.01` Eval-runner (CLI + GUI-knop), resultaten per versie in DB
- [x] `W06.02` Eval-sets: redeneren, code (met tests), geheugen-recall, research/bronnen, toolgebruik, conflicten — start klein, groeit per wave — 7 suites, 34 taken (NL) + retrieval-set; tools/research `requires` W08/W10 → skip tot die er zijn; groeit per wave
- [x] `W06.03` Retrieval-eval: recall@k, rang, irrelevant, geen-antwoord-vragen
- [x] `W06.04` Lekkagecontrole (inhoud + herkomst, near-duplicates)
- [x] `W06.05` Trace-viewer (spans per run), wachttijd vs uitvoering, inference-metrics
- [x] `W06.06` Context Assembly Inspector + "waarom opgehaald"
- [x] `W06.07` Feedback-knoppen in chat → events
- [x] `W06.08` Session replay uit event log
- [x] `W06.09` Error analysis-overzicht, tool/memory analytics
- [x] `W06.10` Dashboard-begin: scores over tijd
- [~] `W06.11` Performance-tests voor alle prestatiedoelen (TTFT, retrieval p95, stop-latency, 60 fps, idle-CPU) — achteruitgang = rood — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag; suite draaien op commando) — harnas `vortex perf` af (korte check OK: retrieval p95 7 ms, stop 13 ms, idle-CPU 0 %) — volledige suite 🔒 op commando
- [~] `W06.12` 24-uurs soak-test met geheugen-telemetrie per proces; vlakke RSS-trend vereist — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag) — harnas `vortex soak --minutes 1440` af (RSS-trend + CSV) — 🔒 24 uur alleen op commando
- [x] `W06.13` Profiling-setup: flamegraphs (Tracy/Superluminal), criterion-benchmarks hete paden — criterion-benches (vectorzoeken, events) + Tracy-feature; bench vond 17,8 ms/100k → geparallelliseerd naar 7 ms

## Gate
Elke code- of configwijziging kan met één actie worden gescoord en vergeleken met de vorige versie.
