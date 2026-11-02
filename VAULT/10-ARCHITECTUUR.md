---
type: architectuur
updated: 2026-11-02
---
# Architectuur

Eén **Rust-kern** is eigenaar van alle taken, geheugen, resources en configversies. Modellen en Python draaien als **aparte processen**, zodat een crash daar de kern en de stopknop nooit meeneemt. Geen microservices.

```
GUI (Tauri + React)  ⇄  events + opdrachten (gRPC/WebSocket, volgnummers)
        │
Rust-kern ── Cognitie · Agents · Scheduler · Geheugen-API · Tools · Neurale layers (ONNX)
        │
 ┌──────┼───────────────┬──────────────────┐
llama-server 5060 Ti   llama-server 1060    Python-worker        Sandbox-procesgroep
(gpt-oss-20b, wissel)  (embed, rerank,      (training, ingest,   (code, shell,
                        router)              parsers, datasets)   risicovolle tools)
        │
SSD (warm): SQLite, LanceDB, index, modellen     HDD (koud): ruwe data, archief, backups
```

## Eigenaarschap
- **Kern:** runstatus, scheduling, cancellation, resourceverdeling, configversies.
- **Opslag:** sessies, taken + afleverstatus, bron- en indexversies, artefactverwijzingen.
- **Workers:** tijdelijke uitvoering, geladen modellen, toolprocessen, voortgang.
- **GUI:** toont toestand, stuurt opdrachten; nooit eigenaar van taken.

## Tech stack
| Laag | Keuze |
| --- | --- |
| Core | Rust: tokio, axum, tonic, serde, tracing |
| Opslag | één SQLite-db (rusqlite, WAL, FTS5) = event log + alle tabellen (migraties 0001–0015); eigen int8-vectorindex; koude laag zstd-blokken |
| Inference | llama.cpp `llama-server` per GPU, OpenAI-compatibele adapter (`vortex-models`), optioneel door de kern beheerd (`managed = true`) |
| Neurale layers | eigen pure-Rust ONNX-runtime (`vortex-neural`), training in de worker (numpy/PyTorch → ONNX) |
| ML-worker | Python 3.12 + uv (`worker/.venv`), gRPC-jobs: parsers/ingest, OCR, spraak, beeld, video, layer- en LoRA-training, HF-datasets, browser (Playwright), compressie |
| Frontend | React + TypeScript + Vite (`app/dist`, door de kern geserveerd op :7420); Tauri 2 voor desktop later |
| Web search | SearXNG (zelf-gehost), crawler met headless browser |
| Werkelijke stand + alle endpoints | [[OVERDRACHT]], [[30-API-EN-CONFIG]] |

## Repo
Repo-root = `vortex/` (GitHub `syneyexx/Vortex`). Actuele kaart met status: [[20-CODEMAP]].
```
vortex/
  core/  (Rust workspace)
    vortex-types/      gedeelde ID's, events, fouten (geen logica)
    vortex-core/       runs, scheduler, event log, config, tracing
    vortex-memory/     MemoryStore-trait, SQLite + LanceDB, tiering
    vortex-cognition/  cyclus, workspace, metacognitie, wereld-/zelfmodel
    vortex-context/    context assembly, budgetten, compactie
    vortex-neural/     ONNX laden/aanroepen, feature logging
    vortex-agents/     debatteam, rollen, conflict-team
    vortex-tools/      toolcontracten, sandbox, MCP, skills
    vortex-models/     registry, lifecycle, templates, parsers, scheduler-hooks
    vortex-server/     eventstream naar GUI, lokale OpenAI-API, vortex CLI
    vortex-crawler/    webcrawler (Scout/Crawler, frontier)
  proto/    gRPC-contracten (zie 15-CONTRACTEN)
  worker/   Python
  app/      Tauri + React
  config/   prompts, rollen, skills, modellen (hot-reload)
  evals/    eval-suite, retrieval-tests, fixtures
  scripts/  install/start/stop (PowerShell), models-up/down, profiling
  tools/    vault.py, api_overview.py, check_deps.py, visual/ (UI-controle)
  install.bat · start_vortex.bat · stop_vortex.bat
  design/   reference-chat.png, tokens
  VAULT/    dit tweede brein
```

Zie [[11-ONE-BRAIN]] voor hoe alles samenhangt.
