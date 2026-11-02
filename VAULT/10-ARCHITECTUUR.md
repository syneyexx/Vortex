---
type: architectuur
updated: 2026-10-31
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
| Opslag | SQLite (rusqlite/sqlx, WAL, FTS5), LanceDB (Rust crate), moka |
| Inference | llama.cpp `llama-server` per GPU, OpenAI-compatibele adapter |
| Neurale layers | ONNX Runtime via `ort` |
| ML-worker | Python 3.11/3.12 + uv, PyTorch, Unsloth/PEFT, datasets, PyMuPDF, ebooklib |
| Frontend | Tauri 2 + React + TypeScript, WebGL-shader |
| Web search | SearXNG (zelf-gehost) |

## Repo
Repo-root = `vortex/` (GitHub `syneyexx/Vortex`). Actuele kaart met status: [[20-CODEMAP]].
```
vortex/
  core/  (Rust workspace)
    vortex-core/       runs, scheduler, event log, config, tracing
    vortex-memory/     MemoryStore-trait, SQLite + LanceDB, tiering
    vortex-cognition/  cyclus, workspace, metacognitie, wereld-/zelfmodel
    vortex-context/    context assembly, budgetten, compactie
    vortex-neural/     ONNX laden/aanroepen, feature logging
    vortex-agents/     debatteam, rollen, conflict-team
    vortex-tools/      toolcontracten, sandbox, MCP, skills
    vortex-models/     registry, lifecycle, templates, parsers, scheduler-hooks
    vortex-server/     eventstream naar GUI, lokale OpenAI-API
  worker/   Python
  app/      Tauri + React
  config/   prompts, rollen, skills, modellen (hot-reload)
  evals/    eval-suite, retrieval-tests, fixtures
  design/   reference-chat.png, tokens
  VAULT/    dit tweede brein
```

Zie [[11-ONE-BRAIN]] voor hoe alles samenhangt.
