---
type: index
updated: 2026-11-02
---
# VORTEX VAULT — START HIER

Tweede brein voor het bouwen van Vortex. Lees dit bestand eerst; open daarna alleen wat de huidige taak nodig heeft.

## Snelstart voor Claude (elke sessie)
1. Lees [[01-REGELS]] (6 werkregels) en [[12-NAVIGATIE]] (navigatieregels N1–N9).
2. Lees [[LOG]] onderaan (laatste 2 sessies): waar zijn we gebleven, welke wave loopt.
3. Open de actieve wave in `waves/` en alleen de modules waar die wave naar linkt. Code zoeken via [[20-CODEMAP]] → `CARD.md` → `vault.py find <ID>`.
4. Na werk: vink items af in de wave, `python tools/vault/vault.py todo`, 3–5 regels in [[LOG]], besluiten in [[03-BESLUITEN]], `vault.py check` groen.

## Shortcuts — "ik moet iets weten over…"
| Vraag | Bestand |
| --- | --- |
| Wat kan Vortex allemaal (per domein) | [[FUNCTIES]] |
| Alle taken met ID + voortgang | [[TODO]] (gegenereerd) |
| Waar staat welke code | [[20-CODEMAP]] |
| Hoe navigeer ik zonder alles te lezen | [[12-NAVIGATIE]] |
| Code-, event- en git-conventies | [[13-CONVENTIES]] |
| Wat betekent term X | [[14-WOORDENLIJST]] |
| Wat moet nog besloten worden | [[OPEN-VRAGEN]] |
| Templates (CARD, wave, besluit, log) | `templates/` |
| Wat is Vortex, waarom, principes | [[02-VISIE]] |
| Wat is al besloten (en waarom) | [[03-BESLUITEN]] |
| Hardware, SSD/HDD-indeling, paden | [[04-HARDWARE-EN-OPSLAG]] |
| Welk model voor welke taak, wisselen | [[05-MODELLEN]] |
| Processen, talen, repo-indeling | [[10-ARCHITECTUUR]] |
| Hoe alles verbonden is (one brain) | [[11-ONE-BRAIN]] |
| Volgorde van bouwen | [[waves/WAVES-OVERZICHT]] |
| Wat bewust NIET nu | [[GEPARKEERD]] |
| Originele input Grok / Astra | `bronnen/` |
| GUI-referentie | `design/reference-chat.png` |

## Modules
- [[modules/runtime-core]] — run state machine, cancellation, event log
- [[modules/orkestrator-performance]] — de Dirigent, hardware-budgetten, nul lekken, prestatiedoelen
- [[modules/inference-modelbeheer]] — registry, lifecycle, templates, wisselen, VRAM
- [[modules/geheugen]] — 7 geheugentypen, schrijven, ophalen, onderhoud, tiering
- [[modules/cognitie]] — cognitieve cyclus, workspace, metacognitie, wereld- en zelfmodel
- [[modules/neurale-layers]] — eigen trainbare netwerken in het framework
- [[modules/context]] — context assembly, budgetten, compactie
- [[modules/redeneren-debat]] — debatteam, judge, fact-checker, conflict-team
- [[modules/tools-skills]] — toolcontracten, sandbox, MCP, skills
- [[modules/research-kennis]] — ingest, crawler-team, wereldmonitor, spelers, datasets
- [[modules/research-team]] — research-pipeline: rollen, loop, datatypen, modi
- [[modules/training]] — vliegwiel, LoRA, DPO, poortwachter
- [[modules/gui]] — design tokens, pagina's, eventstream
- [[modules/observability-eval]] — tracing, eval-suite, tests
- [[modules/autonomie]] — idle-scheduler, leeragenda, briefing, API, telefoon

## Harde afspraken (nooit vergeten)
- Trading pas bij letterlijk **"TRADING NU MAKEN"**.
- Remmen/beperkingen pas bij **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**.
- Gate groen = álles af tot elk detail.
- Gebruiker schrijft Nederlands; antwoorden in het Nederlands.
