---
wave: W12
status: af  # dreaming-gate groen in test; 🔒 TB-simulatie op commando (vortex tierbench)
modules: [geheugen, cognitie]
---
# W12 — Geheugen gevorderd
Zie [[modules/geheugen]], [[modules/cognitie]].

- [x] `W12.01` Procedural memory (skills + werkwijzen, succesrate, versies) — `sync_procedures`: skills + geleerde werkwijzen als Procedural-herinneringen met succesrate, gebruik en versie (nieuwe versie = oude verouderd); Code/Planning-vragen geven werkwijzen voorrang
- [x] `W12.02` Meta-memory (wat bekend, hoe zeker, gaten) → leeragenda — `meta_report`: per soort aantal + zekerheid, verouderd/koud/conflicten/afgeleid, sterkste onderwerpen, kennisgaten (vaak genoemd, weinig feiten) → leeragenda; tab "Wat weet Vortex"
- [x] `W12.03` Dreaming in idle: consolidatie, clustering/merging, decay/pruning, hiërarchische tijd-samenvattingen, reflectie, contradiction resolution, confidence-updates — `dream`: bijna-duplicaten samenvoegen, dag→week→maand-samenvattingen, decay, reflectie, redeneren + consistentie, vertrouwen ↑ bij onafhankelijke steun / ↓ bij tegenspraak; idle elke ~6 u (P3) + knop; gate-test: retrieval (MRR) beter na dromen
- [x] `W12.04` Temporal knowledge graph: validity from/to, historie — feiten met effectieve geldigheid (edge of herinnering → conflictbeslissingen/rollback werken door); `facts_about(entity, at)`, `fact_history`
- [x] `W12.05` Graph-redeneren (afleidingen, consistentie, regels) — regelmotor zonder LM (transitief, symmetrisch, invers, ketens; `config/rules.toml`), afgeleide feiten met regel + steun, opnieuw berekend; functionele relaties met overlappende waarden → conflict
- [x] `W12.06` Zelfmodel uit eval-resultaten → router/metacognitie — `SelfModel` uit de laatste eval-run per suite (taaktype via router) → metacognitie (zwak = diep pad + extra zorg) + leeragenda `weak_task`; ververst na elke eval
- [~] `W12.07` Tiering SSD/HDD: zstd-blokken, ophalen + SSD-cache, prefetch, decay-gestuurd; test met synthetische TB-schaal — `tiering`: decay-gestuurd (W11-layer of heuristiek) naar zstd-blokken op de koude schijf, stub op SSD, centroid-zoeken, SSD-cache (LRU) + prefetch, treffers terughalen; 🔒 `vortex tierbench --memories N` voor de TB-simulatie (op commando)
- [x] `W12.08` Multi-modaal geheugen (afbeeldingen, bestanden als referentie met embeddings) — `POST /api/memory/media`: afbeelding/audio/video/bestand als referentie met beschreven inhoud (geëmbed), bestand ophalen via `/api/memory/:id/media`; beeld-embeddings (CLIP) volgen in W16
- [x] `W12.09` **Experiment (gebruiker wil testen):** quantum-achtige kansmodellen (niet-klassieke kansregels, volgorde-effecten, interferentie tussen hypotheses) voor onzekerheid, tegenstrijdige bronnen en contradiction resolution; A/B tegen gewone Bayesiaanse aanpak op conflict-eval — `belief`: Bayes vs kwantum-achtig (gedeeltelijke projecties met contexthoeken: volgorde-effecten, interferentie, verzadiging bij kopieën); A/B op synthetische conflict-scenario's (onafhankelijk/kopieën/verouderd), θ/λ op trainsplit; uitslag: kwantum beter bij gekopieerde bronnen, Bayes bij onafhankelijke; dashboard-knop
- [x] `W12.10` Memory-based few-shot (voorbeelden uit geheugen in context) — 👍 op een antwoord → voorbeeld in procedural memory; vergelijkbare vragen krijgen 1–2 voorbeelden in de context

## Gate
🔒 (TB-simulatie op commando) Zoeklatency blijft binnen doel bij gesimuleerde 3–4 TB; dreaming verbetert retrieval-eval.
