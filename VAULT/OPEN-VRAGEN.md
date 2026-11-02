---
type: brainstorm
updated: 2026-11-02
---
# Open vragen en brainstorm

Besluit genomen → regel in [[03-BESLUITEN]] en hier naar "Beantwoord".

## Open
- [ ] **PC-details:** moederbord (PCIe-slots: welke GPU op x16?), voeding (genoeg voor 5060 Ti + 1060 onder trainingslast?), Windows-versie, HDD-groottes + schijfletters.
- [ ] **RAM-budget** (model-RAM-cache 12 GB vs worker vs DB-caches) — vastzetten na metingen in W00/W04.
- [ ] **250 GB SSD is krap** (~60–70 GB modellen + index + training-basismodel) — bewaken vanaf W00; zo nodig modelarchief naar HDD.
- [ ] **Versies controleren vóór installeren:** CUDA 12.8/12.9 (sm_61 + sm_120), PyTorch-wheel voor Pascal, llama.cpp-build met `--moe-cache-mib` en NVFP4.

- [ ] **Vectorindex: LanceDB of eigen index?** (2026-11-02) Besloten was LanceDB. De Rust-crate trekt datafusion/arrow binnen en vraagt `protoc` bij het bouwen — zwaar op Windows. Nu gebouwd: eigen int8-index in SQLite (zelfde transactie als de herinnering → vergeten/crash waterdicht), exact zoeken, verwacht < 150 ms tot ~2–3 mln vectoren. Achter `VectorIndex`-trait, dus LanceDB of een ANN (HNSW/IVF) kan er in W12 alsnog achter voor TB-schaal. Akkoord?
- [ ] **"Vergeet dit" en het event log** (2026-11-02): geheugen-events bevatten geen inhoud, maar chatberichten (`chat.message.received`/`delta`) staan wél in het append-only log. Echt wissen daarvan (crypto-shredding of log-compactie) hoort bij de ethics & security layer — of eerder?

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
| 13 | Rookproef-eval vanaf W00 / testen | nee; **regel 7**: kleine tests tussendoor wel, grote tests (24 u, weekruns, suites) alleen op commando |

## Open sinds 2026-11-02 (installatie op de PC)
- **pnpm-stap in `install.bat` faalde** bij de eerste run — oorzaak onbekend (corepack zonder adminrechten? PATH? lockfile?). Installer heeft nu een npx-/lockfile-terugval; `install.log` bekijken.
- Welke modelbestanden precies (quant, bron) — zie [[05-MODELLEN]]; nog niet gedownload.
- `managed = true` (kern beheert llama-server) of `models-up.ps1` als standaard?
