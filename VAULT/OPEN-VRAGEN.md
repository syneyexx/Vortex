---
type: brainstorm
updated: 2026-11-02
---
# Open vragen en brainstorm

Besluit genomen → regel in [[03-BESLUITEN]] en hier naar "Beantwoord".

## Open
- [ ] **Testen "pas als laatst" (antwoord 13, 2026-11-02) — wat precies?** Afgewezen is in elk geval de rookproef-eval vanaf W00. Onduidelijk: geldt het ook voor unit-tests per item? Dat botst met regel 5 en de Definition of Done in [[01-REGELS]] ("tests geschreven en groen"). Tot de gebruiker antwoordt: unit-/integratietests blijven per item (goedkoop, vangen fouten vroeg); grote eval-suites en soak-tests op hun geplande plek.
- [ ] **PC-details:** moederbord (PCIe-slots: welke GPU op x16?), voeding (genoeg voor 5060 Ti + 1060 onder trainingslast?), Windows-versie, HDD-groottes + schijfletters.
- [ ] **RAM-budget** (model-RAM-cache 12 GB vs worker vs DB-caches) — vastzetten na metingen in W00/W04.
- [ ] **250 GB SSD is krap** (~60–70 GB modellen + index + training-basismodel) — bewaken vanaf W00; zo nodig modelarchief naar HDD.
- [ ] **Versies controleren vóór installeren:** CUDA 12.8/12.9 (sm_61 + sm_120), PyTorch-wheel voor Pascal, llama.cpp-build met `--moe-cache-mib` en NVFP4.

## Beantwoord (2026-11-02)
| # | Vraag | Antwoord → besluit |
| --- | --- | --- |
| 1 | Waar de repo? | SSD (`C:\dev\Vortex`); grote data op de HDD's (TB) |
| 2 | Windows of WSL2? | Claude kiest: kern + GUI native Windows; training testen native vs WSL2 in `W00.06`. GUI exact volgens referentie |
| 3 | SSD-type | NVMe 250 GB; rest HDD's |
| 4 | `vortex-types` | ja |
| 5 | `vortex-crawler` | ja |
| 6 | `proto/` | ja, en vastgelegd in de VAULT → [[15-CONTRACTEN]] |
| 7 | W01 splitsen | ja → W01 runtime kern + nieuwe [[waves/W04-dirigent-performance]]; latere waves +1 |
| 8 | Verticale plak eerst | ja → W00–W03 = chat end-to-end |
| 9 | TTFT < 1 s | geen vaste 1 s; meten in W00 en dan vastzetten; Deep toont direct voortgang |
| 10 | Debatteam traag op één GPU | meten in W09; Solvers parallel via slots in `llama-server` |
| 11 | `vortex` CLI | ja → `W00.15` |
| 12 | `justfile` | ja → `W00.14` |
| 13 | Rookproef-eval vanaf W00 | nee — "testen pas als laatst" (zie Open) |
