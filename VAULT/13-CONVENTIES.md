---
type: regels
updated: 2026-11-02
---
# Conventies (code, events, git)

Voorstel 2026-11-02 — bevestigen bij start W00 (zie [[OPEN-VRAGEN]]). Doel: overal hetzelfde patroon, zodat je één crate leest en de rest begrijpt.

## Code
- **Rust:** edition 2024, `clippy -D warnings`, `rustfmt`. Fouten: `thiserror` in libraries, `anyhow` alleen in binaries. Geen `unwrap()`/`expect()` buiten tests (behalve bewezen invarianten met comment).
- **Async:** tokio; elk kanaal begrensd; elke spawned task heeft een eigenaar ([[modules/orkestrator-performance]]).
- **Python:** 3.12, uv, `ruff` + `ruff format`, type hints, `pyright` basic.
- **TS:** strict, eslint + prettier, geen `any`.
- **Publieke API** per crate in `lib.rs` (re-exports); interne modules `pub(crate)`.
- **ID's:** getypeerde newtypes (`RunId`, `MemoryId`, `EventId`, …) in `vortex-types`, ULID-gebaseerd (sorteerbaar op tijd).
- **Tijd:** altijd UTC in opslag; lokale tijd alleen in GUI.
- **Config:** alles wat je wilt tunen in `config/`, niet hardcoded; elke config heeft een versie.

## Events (event log = ruggengraat)
- Naam: `domein.onderwerp.werkwoord_verleden`, bv. `run.state.changed`, `memory.item.written`, `tool.call.completed`, `debate.verdict.given`.
- Elk event: `event_id`, `run_id?`, `trace_id`, `span_id`, `ts`, `type`, `schema_version`, `provenance` (herkomstlabel), `payload`.
- Beslis-events bevatten ook `features`, `choice`, `outcome?` (trainingsdata, [[modules/training]]).
- **Register:** alle event-typen staan als enum in `vortex-types::events` — dat ís de lijst. Nieuw event = daar toevoegen + versie.

## Tests
- Unit-tests naast de code; integratietests in `crate/tests/`; eval in `evals/`.
- Elke prestatie-doelstelling = performance-test ([[modules/orkestrator-performance]]).
- Testnaam beschrijft gedrag: `cancel_during_tool_call_stops_subprocess`.

## Git
- Branch per wave of per item: `w04/hybrid-retrieval`.
- Commit-bericht: `W05.07: hybride ophalen met MMR` — item-ID vooraan, zodat `git log --grep W05.07` alles toont.
- Vóór commit: `cargo fmt && cargo clippy && cargo test`, `ruff`, `vault.py check`.

## VAULT
- Notities Nederlands, code + identifiers Engels.
- Elke notitie heeft frontmatter (`type`, `updated`).
- Links als wikilinks met pad, bv. [[modules/geheugen]] (Obsidian-compatibel); `vault.py check` vangt dode links.
- Besluiten altijd in [[03-BESLUITEN]] (één regel); groot besluit → eigen notitie via `templates/BESLUIT.md`.
