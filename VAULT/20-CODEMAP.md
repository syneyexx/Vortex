---
type: codemap
updated: 2026-11-02
---
# Codemap — waar staat wat

**Status:** gepland (nog geen code). Bijwerken zodra een crate/map echt bestaat (kolom "Bestaat"). Elke rij heeft een `CARD.md` in die map ([[12-NAVIGATIE]] N2).

## Rust-workspace `core/`
| Crate | Verantwoordelijk voor | VAULT-module | Waves | Bestaat |
| --- | --- | --- | --- | --- |
| `vortex-types` | gedeelde ID's, event-typen, fouttypen, tijd, herkomstlabels (geen logica) | [[11-ONE-BRAIN]] | W01 | nee |
| `vortex-core` | runs, state machine, scheduler (W01) + Dirigent (W04), event log + outbox, config, tracing, secrets, supervisie | [[modules/runtime-core]], [[modules/orkestrator-performance]] | W01, W04 | nee |
| `vortex-models` | registry, `Backend`-trait, lifecycle, templates, toolcall-parsers, wisselen, metingen | [[modules/inference-modelbeheer]] | W02 | nee |
| `vortex-server` | eventstream naar GUI, opdrachten, lokale OpenAI-API, `vortex` CLI (admin) | [[modules/gui]], [[modules/autonomie]] | W03, W15 | nee |
| `vortex-memory` | `MemoryStore`-trait, SQLite + LanceDB, schrijf-/ophaalpipeline, tiering | [[modules/geheugen]] | W05, W12 | nee |
| `vortex-eval` *(of `evals/` runner)* | eval-runner, scores per versie, lekkagecontrole | [[modules/observability-eval]] | W06 | nee |
| `vortex-cognition` | cyclus, Global Workspace, doelstapel, metacognitie, wereld-/zelfmodel | [[modules/cognitie]] | W07, W12 | nee |
| `vortex-context` | context assembly, budgetten, compactie, inspector | [[modules/context]] | W07 | nee |
| `vortex-tools` | toolregistry, contracten, sandbox, MCP-client, skills | [[modules/tools-skills]] | W08 | nee |
| `vortex-agents` | debatteam, research-team, conflict-team, rollen | [[modules/redeneren-debat]], [[modules/research-team]] | W09, W10 | nee |
| `vortex-neural` | ONNX via `ort`, feature logging, hot-swap van layers | [[modules/neurale-layers]] | W11 | nee |
| `vortex-crawler` | Scout/Crawler (reqwest), frontier, per-domein tempo | [[modules/research-kennis]] | W10, W14 | nee |

## Overige mappen
| Map | Inhoud | Waves | Bestaat |
| --- | --- | --- | --- |
| `worker/` | Python (uv): ingest, parsers, training, datasets, layer-training → ONNX | W10, W11, W13, W14 | nee |
| `app/` | Tauri 2 + React + TS, design tokens, WebGL-vortex | W03+ | nee |
| `config/` | `models.toml`, `prompts/`, `roles/`, `skills/` (hot-reload) | W00+ | nee |
| `evals/` | eval-sets, fixtures, retrieval-tests | W06+ | nee |
| `proto/` | gRPC-contracten kern ⇄ worker ⇄ GUI ⇄ CLI — kaart: [[15-CONTRACTEN]] | W00, W03, W10 | nee |
| `justfile` | vaste commando's (build, test, check, run, eval, models-up/down) | W00 | nee |
| `design/` | `reference-chat.png`, tokens | W03 | in `VAULT/design/` |
| `tools/` | ontwikkeltools (o.a. `tools/vault/vault.py`) | — | **ja** |
| `VAULT/` | dit tweede brein | — | **ja** |

## Afhankelijkheidsrichting (nooit omgekeerd)
```
vortex-types  ←  alles
vortex-core   ←  models, memory, context, cognition, tools, agents, neural, crawler, server
memory, models, tools, neural  ←  context  ←  cognition  ←  agents  ←  server
```
Een crate lager in de lijst mag nooit een hogere importeren. Cyclus nodig? Dan hoort het type in `vortex-types` of er moet een trait in de lagere crate komen.

> Toegevoegd t.o.v. het oorspronkelijke ontwerp: `vortex-types`, `vortex-crawler`, `proto/`, `vortex` CLI — bevestigd 2026-11-02 ([[03-BESLUITEN]]). Contracten: [[15-CONTRACTEN]].
