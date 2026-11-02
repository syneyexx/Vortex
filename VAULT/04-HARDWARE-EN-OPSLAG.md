---
type: referentie
updated: 2026-10-31
---
# Hardware en opslag

## Machine
| Onderdeel | Spec | Rol in Vortex |
| --- | --- | --- |
| GPU 1 | Inno3D RTX 5060 Ti 16 GB | Hoofdmodel + specialisten (wisselen), training |
| GPU 2 | Asus GTX 1060 6 GB (Pascal) | Altijd geladen: embedding, reranker, router/extractie-model; kleine neurale layers |
| CPU | Ryzen 9 5950X, 16 cores / 32 threads (2 CCD's) | CCD0 interactief, CCD1 achtergrond; MoE-offload, STT/TTS, parsing, ONNX — zie [[modules/orkestrator-performance]] |
| RAM | 32 GB | MoE-experts, modelcache (concurreren met elkaar!), Python-worker |
| SSD | **NVMe**, 250 GB (bevestigd 2026-11-02) | Warm: code (`C:\dev\Vortex`), actieve modellen, DB's, index |
| HDD | meerdere HDD's, TB-schaal (groottes + letters nog noteren) | Koud: ruwe data, volledige teksten, archief, backups |

**Gebruiker (2026-11-02): geen E:-schijf.** Config wijst nu naar D:; ontbrekende mappen geven geen fout (terugval `data`/`data\cold`), aanpassen in GUI → Instellingen → Mappen.

**Open checks (W00):** Staat Windows op de SSD (vrije ruimte)? HDD-groottes + schijfletters? Moederbord/PCIe-slots (welke GPU op x16)? Voeding? Windows-versie? llama.cpp-build die Pascal (1060) én Blackwell (5060 Ti) ondersteunt. Beide GPU's zichtbaar in `nvidia-smi`.

## Indeling SSD (warm) — ~15–20% vrij houden
- `vortex/` code + runtime + config
- Actieve modellen (~60–70 GB, zie [[05-MODELLEN]])
- SQLite (WAL) + LanceDB: warme herinneringen, compacte index + samenvattingen van ALLE kennis
- Caches; tijdelijk trainingsbasismodel tijdens QLoRA

## Indeling HDD (koud)
- Volledige teksten en oude herinneringen, zstd, grote sequentiële blokken
- Ruwe data: ebooks, documenten, HF-datasets (`HF_HOME` op HDD)
- Modelarchief (zelden gebruikt → terugkopiëren)
- Trainingscheckpoints, oude adapters, trace-archief
- Backups van het geheugen (andere schijf = goed)

## Schaal-truc: index op SSD, inhoud op HDD
Zoeken gebeurt altijd op de SSD (int8-vectoren, evt. 512 dims, metadata, graph, samenvattingen). Alleen gevonden items worden van HDD geladen en daarna op SSD gecachet. Tiering automatisch via decay: vaak gebruikt → SSD, lang stil → HDD. Prefetch bij een onderwerp.

Ruwe schatting: ~10 KB per herinnering → 1 mln ≈ 10 GB; 1 ebook ≈ 5 MB verwerkt.

Alle paden instelbaar in Instellingen. Zie [[modules/geheugen]].
