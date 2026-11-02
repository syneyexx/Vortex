---
type: overdracht
updated: 2026-11-02
---
# Overdracht — stand van zaken + opdracht voor de volgende chat

> Lees dit na [[00-START]], [[01-REGELS]] en [[12-NAVIGATIE]]. Hiermee begrijp je het hele project
> zonder de broncode te lezen. Code pas openen via [[20-CODEMAP]] → `CARD.md` van de crate → `vault.py find <ID>`.

## 1. Waar staan we (2026-11-02)
- **Alle waves W00–W18 zijn gebouwd en getest in de cloud** (Linux-container, geen GPU, geen Windows).
  W19 (ethics/security) en W20 (trading) zijn bewust **niet** gebouwd — alleen op letterlijk commando.
- Wat in de cloud niet kon, staat als `[~]` in de waves en in [[PC-VERIFICATIE]]: echte modellen/GPU,
  Windows-specifieke delen, live internetbronnen, Hugging Face, Tailscale, computer use, metingen.
- 🔒 Grote tests (24 u soak, weekrun, `vortex faults`, `vortex ablate`, tierbench, perf) zijn gebouwd maar
  **niet gedraaid** — alleen na "GROTE TESTEN NU" (regel 7).
- Laatste stap van de vorige chat: `install.bat`/`start_vortex.bat` gemaakt; de gebruiker installeert op
  de PC. Eerste run gaf: (a) geen schijf E: → opgelost (melding i.p.v. fout, paden aanpasbaar in de GUI
  onder **Instellingen → Mappen**, kern valt terug op `data`/`data\cold`), (b) **pnpm-stap faalde** →
  installer robuuster gemaakt (npx-terugval, lockfile-terugval) maar **oorzaak nog onbekend** — eerst
  `install.log` bekijken, (c) modelbestanden ontbreken nog (moeten gedownload worden).
- Beslissingen van de gebruiker in de laatste sessie: **geen mobiele weergave**, **geen quantum-tool**
  (gebouwd en weer verwijderd), alle W18-onderdelen standaard uit (`config/frontier.toml`).

## 2. Hoe Vortex in elkaar zit (kort)
```
GUI (React/Vite, app/dist; Tauri later)  ⇄  REST + WebSocket-events  ⇄  Rust-kern `vortex serve` (:7420)
                                                                          │
     llama-server(s) per GPU (OpenAI-API)  ←──── modellen ───────────────┤
     Python-worker (gRPC :50061): ingest, parsers, OCR, spraak, beeld,   ←┤  jobs via gRPC `Train(job_kind)`
       video, training (layers/LoRA), HF-datasets, browser, compressie    │
     Sandbox (procesgroep, limieten): python/shell/tool_program          ←┘
Opslag: één SQLite-database (`data/vortex.db`, WAL) = event log + alles; koude data in `cold_dir`.
```
- **Eén event log** (append-only, `events`-tabel) is de waarheid; alles wat gebeurt is een event
  (`vortex-types/src/event.rs`). GUI leest live mee. Migraties: `vortex-core/migrations/0001–0015`.
- **Dirigent** (`vortex-core`): elke taak is een run met prioriteit P0 (chat) … P3 (achtergrond, alleen
  als de gebruiker idle is); GPU-leases met preemption; drukmeters (RAM/VRAM/idle/game) via de sampler.
- **Chat-pad** (`vortex-server/src/chat.rs` → `cognitive.rs`): bericht → run → bijlagen verwerken (W16) →
  cognitieve cyclus (8 fasen: waarnemen, aandacht/router, herinneren, overwegen/metacognitie,
  handelen [tools, debatteam, research, planning], verifiëren, reflecteren, consolideren) → stream →
  geheugen. Semantische cache (W17.11) kan de Act-fase overslaan.
- **Geheugen** (`vortex-memory`): SQLite + FTS5 + eigen int8-vectorindex, hybride ophalen (RRF),
  reranker, temporele kennisgraaf, conflicten, dromen, tiering naar koud, associatief geheugen (W18, uit).
- **Neurale layers** (`vortex-neural`): eigen pure-Rust ONNX-runtime, 10 kleine netten (router, salience,
  importance, …, crawl_rank) die in schaduw draaien en alleen promoveren als ze de heuristiek verslaan.
- **Agents** (`vortex-agents`): debatteam (Judge-gate), conflict-team, research-team, wereldmodel
  (hypotheses, voorspellingen + kalibratie, spelers), teamgeheugen (W18).
- **Autonomie** (`vortex-server/src/autonomy/`): taakqueue + triggers (cron/interval/event) → executor;
  briefing 07:30, backup 03:15 (+ restore-test), leeragenda, monitoring → suggesties; GPU-bewaker.
- **Experimenten** (`experiments.rs`): A/B, schaduw, metamorf, positioneel, compressie, assoc, prompt-opt,
  ablation (🔒). Zie [[30-API-EN-CONFIG]] voor alle endpoints en commando's.

## 3. Installeren en starten (Windows)
- `install.bat` → `scripts/install.ps1` (winget-programma's, `cargo build --release`, worker-venv via
  `uv sync` met extra's, Playwright-Chromium, optioneel CUDA-PyTorch, `pnpm install` + `pnpm build`,
  mappen/modellen controleren; log `install.log`). Opties `-Gpu`, `-NoGpu`, `-SkipTools`.
- `start_vortex.bat` → `scripts/start.ps1` (worker, `scripts/models-up.ps1` tenzij `managed = true`,
  kern met `VORTEX_EVALS`, browser op http://127.0.0.1:7420). `stop_vortex.bat` stopt alles.
- Paden: `config/paths.toml` of GUI **Instellingen → Mappen**. Modellen: `config/models.toml`.
- **Nooit op Windows uitgevoerd door Claude** — verwacht kleine Windows-fouten bij de eerste runs.

## 4. Opdracht voor de volgende chat
Doel: **Vortex draait op de PC van de gebruiker**, daarna stap voor stap PC-verificatie.
1. **Installatie afmaken.** Vraag de gebruiker om de relevante stukken van `install.log` (vooral de
   pnpm-stap) of lees ze zelf als je lokaal draait. Los Windows-fouten op in `scripts/*.ps1` (UTF-8 mét
   BOM houden; Windows PowerShell 5.1-compatibel: geen `&&`, geen ternary, `${var}:` in strings).
2. **Modellen.** Help de juiste GGUF's te kiezen/downloaden (zie [[05-MODELLEN]] en `config/models.toml`):
   gpt-oss-20b (mxfp4), Qwen3-Embedding-0.6B (Q8), Qwen3-Reranker-0.6B (Q8), Qwen3.5-2B (Q8, router).
   Check `llama_server` in `paths.toml` (installer probeert `winget ggml.llamacpp`).
3. **Eerste start** met `start_vortex.bat`: kern + worker + GUI; chat end-to-end; statusbalk groen.
4. **PC-verificatie** volgens [[PC-VERIFICATIE]] (fase A → E), per item afvinken in de wave (`[~]` →
   `[x]`) met meetwaarden in [[LOG]]/[[05-MODELLEN]].
5. Grote tests alleen op commando. W19/W20 niet beginnen zonder het exacte commando.

## 5. Bekende aandachtspunten
- pnpm-fout bij de eerste installatie: oorzaak onbekend (zie §1).
- `managed = true` (kern beheert llama-server zelf, GPU vrijgeven bij gamen) staat nog uit; standaard
  start `models-up.ps1` de servers met vaste argumenten — controleer poorten 8080–8083 tegen models.toml.
- Tauri-desktopapp is niet gebouwd door de installer (GUI draait in de browser via de kern); `pnpm tauri
  build` kan later.
- Hardware-aanname: 5060 Ti (sm_120, CUDA 12.8+) + 1060 (sm_61, int8). CUDA 13 ondersteunt de 1060 niet.
- Datamap standaard `data/` in de repo; HDD optioneel (geen E:-schijf bij de gebruiker).

## 6. Werkwijze (onveranderd)
Na elk stuk werk: wave-items afvinken, `python tools/vault/vault.py todo`, 3–5 regels in [[LOG]],
besluiten in [[03-BESLUITEN]], CARD.md bij API-wijziging, `python tools/vault/api_overview.py` bij
route/CLI/config-wijziging, `python tools/vault/vault.py check` groen, commit begint met item-ID.
Checks: `just check` (fmt, clippy -D warnings, ruff, eslint/prettier, deps, vault) en `just test`.
