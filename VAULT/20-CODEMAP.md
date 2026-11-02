---
type: codemap
updated: 2026-11-02
---
# Codemap — waar staat wat

**Status (2026-11-02):** alle crates bestaan (skelet), `vortex-types` + `vortex-core` + `vortex-proto` + `vortex-server` (CLI) zijn gebouwd. Bijwerken zodra een crate/map echt bestaat (kolom "Bestaat"). Elke rij heeft een `CARD.md` in die map ([[12-NAVIGATIE]] N2).

## Rust-workspace `core/`
| Crate | Verantwoordelijk voor | VAULT-module | Waves | Bestaat |
| --- | --- | --- | --- | --- |
| `vortex-proto` | gegenereerde gRPC-code uit `/proto` | [[15-CONTRACTEN]] | W00 | **ja** |
| `vortex-types` | gedeelde ID's, event-typen, fouttypen, tijd, herkomstlabels (geen logica) | [[11-ONE-BRAIN]] | W01 | **ja** |
| `vortex-core` | runs, state machine, scheduler (W01) + Dirigent (W04), event log + outbox, config, tracing, secrets, supervisie | [[modules/runtime-core]], [[modules/orkestrator-performance]] | W01, W04 | **ja** |
| `vortex-models` | registry, `Backend`-trait, lifecycle, templates, toolcall-parsers, wisselen, metingen | [[modules/inference-modelbeheer]] | W02 | skelet |
| `vortex-server` | eventstream naar GUI, opdrachten, lokale OpenAI-API, `vortex` CLI (admin) | [[modules/gui]], [[modules/autonomie]] | W03, W15 | **ja** (CLI) |
| `vortex-memory` | `MemoryStore`-trait, SQLite + LanceDB, schrijf-/ophaalpipeline, tiering | [[modules/geheugen]] | W05, W12 | skelet |
| `vortex-eval` *(of `evals/` runner)* | eval-runner, scores per versie, lekkagecontrole | [[modules/observability-eval]] | W06 | skelet |
| `vortex-cognition` | cyclus, Global Workspace, doelstapel, metacognitie, wereld-/zelfmodel | [[modules/cognitie]] | W07, W12 | skelet |
| `vortex-context` | context assembly, budgetten, compactie, inspector | [[modules/context]] | W07 | skelet |
| `vortex-tools` | toolregistry, contracten, sandbox, MCP-client, skills | [[modules/tools-skills]] | W08 | skelet |
| `vortex-agents` | debatteam, research-team, conflict-team, rollen | [[modules/redeneren-debat]], [[modules/research-team]] | W09, W10 | skelet |
| `vortex-neural` | ONNX via `ort`, feature logging, hot-swap van layers | [[modules/neurale-layers]] | W11 | skelet |
| `vortex-crawler` | Scout/Crawler (reqwest), frontier, per-domein tempo | [[modules/research-kennis]] | W10, W14 | skelet |

## Overige mappen
| Map | Inhoud | Waves | Bestaat |
| --- | --- | --- | --- |
| `worker/` | Python (uv): ingest, parsers, training, datasets, layer-training → ONNX | W10, W11, W13, W14 | **ja** (skelet + gRPC) |
| `app/` | Tauri 2 + React + TS, design tokens, WebGL-vortex | W03+ | skelet |
| `config/` | `models.toml`, `prompts/`, `roles/`, `skills/` (hot-reload) | W00+ | **ja** |
| `evals/` | eval-sets, fixtures, retrieval-tests | W06+ | leeg |
| `proto/` | gRPC-contracten kern ⇄ worker ⇄ GUI ⇄ CLI — kaart: [[15-CONTRACTEN]] | W00, W03, W10 | **ja** |
| `justfile` | vaste commando's (build, test, check, run, eval, models-up/down) | W00 | **ja** |
| `design/` | `reference-chat.png`, tokens | W03 | in `VAULT/design/` |
| `scripts/` | `models-up.ps1`, `models-down.ps1` (llama-servers) | W00 | **ja** |
| `tools/` | ontwikkeltools: `vault/vault.py`, `check_deps.py` | — | **ja** |
| `VAULT/` | dit tweede brein | — | **ja** |

## Afhankelijkheidsrichting (nooit omgekeerd)
```
vortex-types, vortex-proto  ←  alles
vortex-core   ←  models, memory, context, cognition, tools, agents, neural, crawler, server
memory, models, tools, neural  ←  context  ←  cognition  ←  agents  ←  server
```
Een crate lager in de lijst mag nooit een hogere importeren. Cyclus nodig? Dan hoort het type in `vortex-types` of er moet een trait in de lagere crate komen.

> Toegevoegd t.o.v. het oorspronkelijke ontwerp: `vortex-types`, `vortex-crawler`, `proto/`, `vortex` CLI — bevestigd 2026-11-02 ([[03-BESLUITEN]]). Contracten: [[15-CONTRACTEN]].
