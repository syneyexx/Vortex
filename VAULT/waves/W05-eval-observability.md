---
wave: W05
status: open
modules: [observability-eval]
---
# W05 — Eval en observability
Zie [[modules/observability-eval]].

- [ ] `W05.01` Eval-runner (CLI + GUI-knop), resultaten per versie in DB
- [ ] `W05.02` Eval-sets: redeneren, code (met tests), geheugen-recall, research/bronnen, toolgebruik, conflicten — start klein, groeit per wave
- [ ] `W05.03` Retrieval-eval: recall@k, rang, irrelevant, geen-antwoord-vragen
- [ ] `W05.04` Lekkagecontrole (inhoud + herkomst, near-duplicates)
- [ ] `W05.05` Trace-viewer (spans per run), wachttijd vs uitvoering, inference-metrics
- [ ] `W05.06` Context Assembly Inspector + "waarom opgehaald"
- [ ] `W05.07` Feedback-knoppen in chat → events
- [ ] `W05.08` Session replay uit event log
- [ ] `W05.09` Error analysis-overzicht, tool/memory analytics
- [ ] `W05.10` Dashboard-begin: scores over tijd
- [ ] `W05.11` Performance-tests voor alle prestatiedoelen (TTFT, retrieval p95, stop-latency, 60 fps, idle-CPU) — achteruitgang = rood
- [ ] `W05.12` 24-uurs soak-test met geheugen-telemetrie per proces; vlakke RSS-trend vereist
- [ ] `W05.13` Profiling-setup: flamegraphs (Tracy/Superluminal), criterion-benchmarks hete paden

## Gate
Elke code- of configwijziging kan met één actie worden gescoord en vergeleken met de vorige versie.
