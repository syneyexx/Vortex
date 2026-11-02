---
type: module
crate: vortex-context
waves: [W07]
updated: 2026-10-31
---
# Context assembly

Context is een beperkt rekenmiddel. De Context Assembler bouwt per stap de prompt uit de Global Workspace.

## Regels
- **Antwoordbudget eerst reserveren** (antwoord, toolaanvragen, afsluiting), daarna bronmateriaal.
- **Dynamisch system prompt management**: system prompt + rol-prompts uit Instellingen (versies), skills/tools alleen naam+beschrijving tot nodig (tool discovery).
- **Diversiteitsbewuste selectie** (MMR) + **gereserveerd tegenbewijs** (relevant, onderbouwd).
- **Verliesbewuste compactie**: exact bewaren = paden, IDs, voorwaarden, bedragen, eenheden, letterlijke gebruikerscorrecties; samenvatbaar = uitleg; weglaatbaar = herhaling.
- **Selectieve contextbewerking**: oude toolresultaten eruit, verwijzing houden.
- **Hiërarchische samenvatting** van lange gesprekken.
- **Geïsoleerde agentcontexten**: elke rol alleen wat nodig is; grote resultaten via bestanden/verwijzingen.
- **Resultaten met code filteren** vóór modelinvoer.
- **Prompt caching** (prefix) waar backend het kan; cache-isolatie per werkruimte.
- **Context Assembly Inspector**: toont instructies, herinneringen, bronnen, toolresultaten en wat wegviel.
- Positionele contexttests per model (W17).

## Gesprekken
Session persistence & restore, conversation branching/forking, multi-turn state, artifact management (code, bestanden, tabellen), project-werkruimtes.
