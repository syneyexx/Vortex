# Vortex — instructies voor Claude

Dit is het project **Vortex**: een persoonlijk, lokaal AI-framework (harness) — Rust-kern, Python ML-worker, Tauri + React GUI. Repo: GitHub `syneyexx/Vortex`. De gebruiker (syneye) schrijft Nederlands; antwoord in het Nederlands. Code en identifiers in het Engels.

## Begin elke sessie zo (lees NIET het hele project)
0. **Nieuwe chat:** eerst `VAULT/OVERDRACHT.md` — stand van zaken, architectuur in het kort, opdracht.
1. `VAULT/00-START.md` — index + shortcuts.
2. `VAULT/01-REGELS.md` (werkregels 1–7) en `VAULT/12-NAVIGATIE.md` (navigatieregels N1–N9).
3. Onderaan `VAULT/LOG.md`: laatste 2 sessies + actieve wave.
4. Alleen de actieve wave in `VAULT/waves/` en de modules waar die naar linkt.
5. Code zoeken: `VAULT/20-CODEMAP.md` → `CARD.md` van de crate → `python tools/vault/vault.py find <item-ID>`.

## Na elk stuk werk
- Vink items af in de wave-file (`[x]`, of `[~]` = bezig), daarna `python tools/vault/vault.py todo`.
- 3–5 regels in `VAULT/LOG.md` (template: `VAULT/templates/SESSIE-LOG.md`).
- Nieuwe besluiten in `VAULT/03-BESLUITEN.md`; open punten in `VAULT/OPEN-VRAGEN.md`.
- `CARD.md` bijwerken als een publieke API veranderde; code-tag `VX:<item-ID>` bij de ingang.
- Routes/CLI/config gewijzigd → `python tools/vault/api_overview.py` (ververst `VAULT/30-API-EN-CONFIG.md`).
- PC-verificatie gedaan → item in de wave `[x]` + meetwaarde in LOG; `VAULT/PC-VERIFICATIE.md` afvinken.
- `python tools/vault/vault.py check` moet groen zijn. Commit-bericht begint met het item-ID (`W04.07: …`).

## Nooit vergeten
- Gate groen = élk item af tot in detail (regel 5).
- Kleine tests altijd; grote tests (24 u soak, weekruns, volledige suites, ablations, fault-injection) alleen op commando, bv. **"GROTE TESTEN NU"** (regel 7, 🔒 in de waves).
- Geen remmen/beperkingen bouwen tot de gebruiker zegt: **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**.
- Trading pas bij **"TRADING NU MAKEN"**.
- Geschrapt door de gebruiker: mobiele weergave, quantum-tool. Niet opnieuw bouwen.
- Windows-scripts (`scripts/*.ps1`): UTF-8 mét BOM, compatibel met Windows PowerShell 5.1.
- One brain: elke module aangesloten op event log, Global Workspace, geheugen en metacognitie (`VAULT/11-ONE-BRAIN.md`).
- Alle taken lopen via de Dirigent (`VAULT/modules/orkestrator-performance.md`).
- `VAULT/TODO.md` is gegenereerd: nooit met de hand bewerken.
