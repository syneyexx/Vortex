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
| CLI: `vortex serve [--listen 127.0.0.1:7420] [--app-dist ../app/dist]`, `vortex status`, `vortex runs` | |

## Events (schrijft)
`chat.message.{received,started,delta,completed}`, `chat.session.{created,forked,updated}`, `settings.prompt.updated`, `model.inference.measured` + alles van `vortex-core`.

## Invarianten
- Antwoordtekst = som van de `chat.message.delta`-events (herladen/crash verliest niets).
- Run leeft los van de WebSocket; alleen `POST /stop` stopt.

## Ingangen
`src/api.rs` · `src/chat.rs` · `src/sessions.rs` · `src/live.rs` · `src/main.rs` (CLI) · schema: `vortex-core/migrations/0002_sessions.sql`

## Tests
`cargo test -p vortex-server` — `tests/api.rs` (7): chat end-to-end, resume zonder gaten, stop midden in stream (+ herladen tijdens streamen), verbinding weg, fork, prompts met versies.
