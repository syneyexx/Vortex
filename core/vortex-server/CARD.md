# CARD — `vortex-server`

**Doel:** het kern-proces: HTTP/WebSocket-API voor de GUI, chat-engine, sessies, live-status, en de `vortex` CLI.
**VAULT-module:** VAULT/modules/gui.md · **Waves:** W00 (CLI), W03 (API + chat), W15 (lokale OpenAI-API)

## Publieke API
| Item | Doet |
| --- | --- |
| `api::build_state(Kernel)` + `api::router(state, app_dist)` | axum-router; herstelt eerst berichten die tijdens een crash streamden |
| `GET /api/events?from_seq=N` (WebSocket) | replay vanaf N + live; elk frame = event-JSON met `seq`; geen gaten; verbinding weg ≠ run gestopt |
| `GET /api/status` | subsystemen, GPU's (nvidia-smi), servers (/health), tok/s-reeks, `last_seq` (snapshot) |
| `/api/sessions` (GET/POST), `/api/sessions/:id` (GET/PATCH/DELETE), `/:id/messages` (POST), `/:id/fork` (POST) | sessies + berichten; streaming-berichten worden aangevuld uit het event log |
| `GET /api/runs`, `POST /api/runs/:id/stop` | lopend werk + stopknop |
| `GET /api/models` | registry + serverstatus |
| `/api/settings/prompts` (GET/PUT), `/api/settings/history`, `/api/settings/restore` | prompts live vanaf volgend bericht, versies, terugzetten; alleen `prompts/`/`roles/*.md` |
| `chat::ChatEngine` | bericht → run P0 → GPU-slot → stream → deltas (40 ms/512 tekens) → afronden (outbox) |
| `sessions::SessionStore` | opslaan/ophalen/forken/archiveren, `repair_streaming` |
| `live::Live` | achtergrondpoller voor de statusbalk |
| `memory_api` | `/api/memory*`: lijst/zoek/bewerk/vergeet/conflicten/werkruimtes/export/import/backup/stats |
| `eval_api` (`VortexSubject`) | `/api/eval/{suites,run,runs,runs/:id,retrieval,leakage}` |
| `observe_api` | `/api/messages/:id/feedback`, `/api/runs/:id/trace`, `/api/sessions/:id/replay`, `/api/analytics` |
| `ChatEngine::answer_once` | één antwoord zonder opslag (eval), P2 via Dirigent |
| CLI: `vortex eval [--suite x] [--retrieval] [--leakage]`, 🔒 `vortex perf [--quick]`, 🔒 `vortex soak --minutes N` | |
| `cognitive.rs`: `ChatEngine::run` — elke chat doorloopt de cognitieve cyclus (W07) | |
| REST W07: `POST /api/runs/:id/steer`, `GET /api/runs/:id/workspace`, `GET/POST /api/agenda[/:id]`, `GET /api/profile` | |
| `tools_api.rs`: hub uit config + geheugen-tools; REST `GET /api/tools`, `POST /api/tools/reload`, `POST /api/tools/:name/call` (dry_run, root), `GET/POST /api/skills…` (import, export, status, verwijderen), `GET/POST /api/skills/proposals[/:id]` | |
| Chat met tools: kernset met schema, toolrondes (max 8) via `Executor::execute_plan`, skills automatisch geladen bij hoge score | |
| `team_api.rs`: rol-LLM's, conflict-team (voorgrond in de chat, achtergrond-worker P3), `POST /api/memory/conflicts/:id/team`, `POST /api/memory/conflicts/:id/rollback` | |
| Deep/moeilijk: debatteam vóór het streamen (Synthesizer); eval `--mode quick|deep` voor de W09-vergelijking | |
| `ingest.rs`: `ChatEngine::ingest` (worker of Rust-fallback → geheugen met locator → samenvattingen, incrementeel) · `research_api.rs`: `POST/GET /api/research`, `GET /api/research/:id[/export?format=csv|jsonl|parquet]`, `POST /api/research/corpus`, `POST /api/ingest`, `GET /api/ingest/documents`; chat: pad Research → research-team | |
| `neural_api.rs`: registry koppelen (`attach`), `build_examples`, `bootstrap`, `train_layer` (worker → install → eval → auto_decide), idle-training, reranker-dataset; REST `GET /api/neural`, `POST /api/neural/:layer/{train,bootstrap,promote,rollback,mode,label}`, `POST /api/neural/reranker/dataset` | |
| `memory_adv.rs`: zelfmodel, dromen + tiering in idle, procedures, few-shot, media; REST `/api/memory/{graph,graph/history,reason,dream,dreams,meta,procedures,tier,tier/demote,media,:id/media,belief-experiment}`, `/api/selfmodel`; 🔒 CLI `vortex tierbench` | |
| `training_api.rs`: `build_{sft,dpo,qa}`, `train_adapter` (worker, idle, pauzeren/hervatten), `set_adapter_scale` (hot-swap), `shadow_compare`, `gate`, `calibrate`, vliegwiel; REST `/api/training`, `/api/training/{datasets/:kind,train,calibrate}`, `/api/training/adapters/:id/{gate,shadow,scale}` | |
| CLI: `vortex serve [--listen 127.0.0.1:7420] [--app-dist ../app/dist]`, `vortex status`, `vortex runs` | |

## Events (schrijft)
`chat.message.{received,started,delta,completed}`, `cognition.*` (fasen, strategie, plan, stappen, verificatie, workspace, salience, agenda), `context.prompt.assembled` (Inspector), `feedback.user.given`, `eval.*`, `chat.session.{created,forked,updated}`, `settings.prompt.updated`, `model.inference.measured` + alles van `vortex-core`.

## Invarianten
- Antwoordtekst = som van de `chat.message.delta`-events (herladen/crash verliest niets).
- Run leeft los van de WebSocket; alleen `POST /stop` stopt.

## Ingangen
`src/api.rs` · `src/chat.rs` · `src/cognitive.rs` · `src/observe_api.rs` · `src/sessions.rs` · `src/live.rs` · `src/main.rs` (CLI) · schema: `vortex-core/migrations/0002_sessions.sql`

## Tests
`cargo test -p vortex-server` — `tests/api.rs` (18): chat end-to-end, resume zonder gaten, stop midden in stream (+ herladen tijdens streamen), verbinding weg, fork, prompts met versies, geheugen, eval, diep pad met plan + kritiek, persoonlijke vraag zonder geheugen, bijsturen midden in een stream, diep pad met debatteam (Judge vóór het antwoord, lokaal bewijs), chat met tools (kaarten, preview, skill-voorstel), ingest + research-API, EPUB via de echte Python-worker (gate W10), neurale layer trainen → promoveren → terugdraaien via de worker (gate W11).
