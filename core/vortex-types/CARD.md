# CARD — `vortex-types`

**Doel:** gedeelde typen zonder logica: ID's, event-register, run-statussen, prioriteiten, herkomstlabels, foutsoorten.
**Waves:** W00/W01. Afhankelijkheden: geen interne.

## Publieke API
| Item | Doet |
| --- | --- |
| `RunId`, `EventId`, `TraceId`, `SpanId`, `MemoryId`, `CheckpointId`, `ResourceId`, `SessionId`, `MessageId`, `ToolCallId`, `WorkspaceId` | ULID-ID's, tekst `prefix_ULID` |
| `EventKind` | **het** event-register (naam `domein.onderwerp.werkwoord`) |
| `Event`, `DecisionRecord` | event-envelop; beslissing + features (trainingsdata) |
| `RunState`, `StopStatus`, `Priority`, `RunKind` | gedeelde statussen |
| `Provenance`, `SourceKind` | herkomstlabels (hook voor latere policies) |
| `ErrorKind`, `VortexError` | gemeenschappelijke foutsoorten |

## Invarianten
- Event-namen uniek en volgens conventie (test). Nieuw event = hier toevoegen.
- Gespiegeld in `proto/vortex/common/v1/common.proto` — samen wijzigen.

## Ingangen
`src/event.rs` (register) · `src/ids.rs` · `src/run.rs`

## Tests
`cargo test -p vortex-types`
