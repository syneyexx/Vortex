---
wave: W06
status: open
modules: [observability-eval]
---
# W06 — Eval en observability
Zie [[modules/observability-eval]].

- [ ] `W06.01` Eval-runner (CLI + GUI-knop), resultaten per versie in DB
- [ ] `W06.02` Eval-sets: redeneren, code (met tests), geheugen-recall, research/bronnen, toolgebruik, conflicten — start klein, groeit per wave
- [ ] `W06.03` Retrieval-eval: recall@k, rang, irrelevant, geen-antwoord-vragen
- [ ] `W06.04` Lekkagecontrole (inhoud + herkomst, near-duplicates)
- [ ] `W06.05` Trace-viewer (spans per run), wachttijd vs uitvoering, inference-metrics
- [ ] `W06.06` Context Assembly Inspector + "waarom opgehaald"
- [ ] `W06.07` Feedback-knoppen in chat → events
- [ ] `W06.08` Session replay uit event log
- [ ] `W06.09` Error analysis-overzicht, tool/memory analytics
- [ ] `W06.10` Dashboard-begin: scores over tijd
- [ ] `W06.11` Performance-tests voor alle prestatiedoelen (TTFT, retrieval p95, stop-latency, 60 fps, idle-CPU) — achteruitgang = rood — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag; suite draaien op commando)
- [ ] `W06.12` 24-uurs soak-test met geheugen-telemetrie per proces; vlakke RSS-trend vereist — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag)
- [ ] `W06.13` Profiling-setup: flamegraphs (Tracy/Superluminal), criterion-benchmarks hete paden

## Gate
Elke code- of configwijziging kan met één actie worden gescoord en vergeleken met de vorige versie.
