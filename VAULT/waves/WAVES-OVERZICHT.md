---
type: waves
updated: 2026-10-31
---
# Bouwplan in waves

Elke wave levert een werkende, geteste Vortex op. **Gate = elk item af tot in detail** (zie Definition of Done in [[01-REGELS]]). Volgende wave start pas als de gate groen is. Elke wave sluit aan op de vier ruggengraten van [[11-ONE-BRAIN]].

| Wave | Naam | Kern | Gate (kort) |
| --- | --- | --- | --- |
| [[W00-fundament]] | Fundament | toolchains, repo, beide llama-servers, registry-basis | beide modellen gereed, metingen gelogd |
| [[W01-runtime-core]] | Runtime core | state machine, event log, cancellation, scheduler | crash/herstart verliest niets |
| [[W02-modelbeheer]] | Modelbeheer | lifecycle, templates, parsers, wisselen | elk model veilig laden/wisselen |
| [[W03-gui-chat]] | GUI + chat | design tokens, chatpagina, stream, instellingen | 1-op-1 referentie, live data |
| [[W04-geheugen-kern]] | Geheugen kern | schrijven, hybrid ophalen, dedup, vergeten | recall-tests groen |
| [[W05-eval-observability]] | Eval + observability | eval-suite, tracing, inspector | elke wijziging gescoord |
| [[W06-cognitie-context]] | Cognitie + context | cyclus, workspace, metacognitie, assembler | System 1/2 kiest correct |
| [[W07-tools-skills]] | Tools + skills | contracten, sandbox, MCP, skills | getypeerde acties veilig herhaalbaar |
| [[W08-debatteam]] | Debatteam | rollen, judge-gate, fact-check, conflict-team | eval stijgt t.o.v. single-agent |
| [[W09-research-ingest]] | Research + ingest | pipeline, ebooks, onderwerp-research | ebook doorzoekbaar met bron |
| [[W10-neurale-layers]] | Neurale layers | feature logging → ONNX-netten | netten winnen van heuristiek |
| [[W11-geheugen-gevorderd]] | Geheugen gevorderd | procedural, meta, dreaming, tiering, graph-redeneren | 3–4 TB-architectuur werkt |
| [[W12-training-vliegwiel]] | Training | SFT, DPO, QLoRA, poortwachter | adapter wint op eval |
| [[W13-wereldkennis]] | Wereldkennis | wereldmonitor, spelers, HF-datasets | voorspellingen getoetst |
| [[W14-autonomie]] | Autonomie | idle, leeragenda, briefing, API, telefoon | draait zelfstandig |
| [[W15-multimodaal]] | Multimodaal | visie, OCR, spraak, beeld | alle modaliteiten in chat |
| [[W16-kwaliteit-performance]] | Kwaliteit + performance | geavanceerde tests, speculative, browser/git | ablations bewijzen waarde |
| [[W17-frontier]] | Frontier | neural memory, continual learning, causaal | meetbare winst |
| [[W18-ethics-security]] | Ethics & security | **alleen op commando** | — |
| [[W19-trading]] | Trading | **alleen op commando** | — |

Huidige wave: zie [[LOG]].
