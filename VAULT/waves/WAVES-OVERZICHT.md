---
type: waves
updated: 2026-10-31
---
# Bouwplan in waves

Elke wave levert een werkende, geteste Vortex op. **Gate = elk item af tot in detail** (zie Definition of Done in [[01-REGELS]]). Volgende wave start pas als de gate groen is. Elke wave sluit aan op de vier ruggengraten van [[11-ONE-BRAIN]].

| Wave | Naam | Kern | Gate (kort) |
| --- | --- | --- | --- |
| [[W00-fundament]] | Fundament | toolchains, repo, beide llama-servers, registry-basis | beide modellen gereed, metingen gelogd |
| [[W01-runtime-core]] | Runtime kern | state machine, event log, cancellation, checkpoints, simpele GPU-scheduler | crash/herstart verliest niets |
| [[W02-modelbeheer]] | Modelbeheer | lifecycle, templates, parsers, wisselen | elk model veilig laden/wisselen |
| [[W03-gui-chat]] | GUI + chat | design tokens, chatpagina, stream, instellingen | 1-op-1 referentie, live data — **chat werkt end-to-end** |
| [[W04-dirigent-performance]] | Dirigent + performance | P0–P3, preemption, CPU/RAM-budgetten, HDD-wachtrij | chat onderbreekt achtergrond, nooit OOM |
| [[W05-geheugen-kern]] | Geheugen kern | schrijven, hybrid ophalen, dedup, vergeten | recall-tests groen |
| [[W06-eval-observability]] | Eval + observability | eval-suite, tracing, inspector | elke wijziging gescoord |
| [[W07-cognitie-context]] | Cognitie + context | cyclus, workspace, metacognitie, assembler | System 1/2 kiest correct |
| [[W08-tools-skills]] | Tools + skills | contracten, sandbox, MCP, skills | getypeerde acties veilig herhaalbaar |
| [[W09-debatteam]] | Debatteam | rollen, judge-gate, fact-check, conflict-team | eval stijgt t.o.v. single-agent |
| [[W10-research-ingest]] | Research + ingest | pipeline, ebooks, onderwerp-research | ebook doorzoekbaar met bron |
| [[W11-neurale-layers]] | Neurale layers | feature logging → ONNX-netten | netten winnen van heuristiek |
| [[W12-geheugen-gevorderd]] | Geheugen gevorderd | procedural, meta, dreaming, tiering, graph-redeneren | 3–4 TB-architectuur werkt |
| [[W13-training-vliegwiel]] | Training | SFT, DPO, QLoRA, poortwachter | adapter wint op eval |
| [[W14-wereldkennis]] | Wereldkennis | wereldmonitor, spelers, HF-datasets | voorspellingen getoetst |
| [[W15-autonomie]] | Autonomie | idle, leeragenda, briefing, API, telefoon | draait zelfstandig |
| [[W16-multimodaal]] | Multimodaal | visie, OCR, spraak, beeld | alle modaliteiten in chat |
| [[W17-kwaliteit-performance]] | Kwaliteit + performance | geavanceerde tests, speculative, browser/git | ablations bewijzen waarde |
| [[W18-frontier]] | Frontier | neural memory, continual learning, causaal | meetbare winst |
| [[W19-ethics-security]] | Ethics & security | **alleen op commando** | — |
| [[W20-trading]] | Trading | **alleen op commando** | — |

## Volgorde-logica (herzien 2026-11-02)
1. **W00–W03 = verticale plak:** zo snel mogelijk echte chat end-to-end (model → kern → GUI, met event log en stopknop).
2. **W04 Dirigent** vóór geheugen/ingest: vanaf dan draait er meer dan één soort werk tegelijk.
3. Daarna elke wave een laag slimmer: geheugen → meten → cognitie → tools → debat → research → leren.
4. **Tests:** zie [[OPEN-VRAGEN]] (vraag over "testen pas als laatst").

Huidige wave: zie [[LOG]].
