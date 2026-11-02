---
type: architectuur
updated: 2026-11-02
---
# Contracten — hoe de processen met elkaar praten

**Bron van waarheid = `proto/` in de repo** (gRPC/protobuf, codegen voor Rust, Python, TS). Deze notitie is de kaart: welke contracten er zijn, wie eigenaar is, waar ze staan. Contract veranderd → hier bijwerken in dezelfde commit.

## Overzicht
| Verbinding | Protocol | Contract | Eigenaar | Wave |
| --- | --- | --- | --- | --- |
| GUI ⇄ kern | WebSocket (events) + gRPC-web/HTTP (opdrachten) | `proto/vortex/gui/v1/*.proto` | `vortex-server` | W03 |
| kern ⇄ Python-worker | gRPC (lokaal, named pipe / localhost) | `proto/vortex/worker/v1/*.proto` | `vortex-core` | W00 skelet, W10 ingest, W13 training |
| kern ⇄ `llama-server` | HTTP, OpenAI-compatibel | geen proto; adapter in `vortex-models` | `vortex-models` | W02 |
| kern ⇄ sandbox | procesgroep + stdin/stdout JSON | `proto/vortex/sandbox/v1/*.proto` | `vortex-tools` | W08 |
| kern ⇄ MCP-servers | MCP (stdio / HTTP) | MCP-spec | `vortex-tools` | W08 |
| buitenwereld ⇄ kern | HTTP, OpenAI-compatibel | lokale API | `vortex-server` | W15 |
| `vortex` CLI ⇄ kern | gRPC | `proto/vortex/admin/v1/*.proto` | `vortex-server` | W00+ |

## Regels
- **Versie in het pad** (`v1`, `v2`): nooit een veld hergebruiken of van type veranderen; alleen toevoegen. Breken = nieuwe versie.
- **Elk bericht draagt** `trace_id` en, waar relevant, `run_id` (one brain: tracing loopt door alle processen).
- **Events naar de GUI** hebben volgnummers; de GUI kan hervatten na reconnect (snapshot + delta's), zie [[modules/gui]].
- **Annuleren is een eersteklas bericht** in elk contract (cancellation door de hele keten, [[modules/runtime-core]]).
- **Grote data nooit in berichten:** verwijzing naar bestand/artefact-ID.
- **Gedeelde typen** (ID's, event-typen) komen uit `vortex-types`, gespiegeld in `proto/vortex/common/v1/`.

## Services (eerste opzet, uitwerken in W00/W01)
- `WorkerService`: `Health`, `Ingest(stream)`, `Parse`, `Embed?` (embedding draait op llama-server 1060), `Train(stream)`, `Cancel`.
- `GuiEvents`: `Subscribe(from_seq)` → stream van getypeerde frames (tekst, toolstart, toolresultaat, artefact, waarschuwing, einde).
- `GuiCommands`: `SendMessage`, `Stop(run_id)`, `Pause`, `SetSetting`, …
- `Admin` (CLI): `Status`, `Models`, `Runs`, `RunEval`, `LoadModel`.
