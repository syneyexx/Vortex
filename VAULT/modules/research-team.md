---
type: module
crate: vortex-agents (research) + worker/ingest
waves: [W10, W14, W17]
updated: 2026-10-31
---
# Research-team (pipeline naast de chat-pipeline)

Zelfde agent-framework en Judge-gate als [[modules/redeneren-debat]], maar gebouwd voor één doel: **van vraag naar geverifieerde kennis én bruikbare data, in één doorlopende stroom.** Alles wat gevonden wordt, wordt direct omgezet en opgeslagen — niet pas aan het eind.

## Rollen
| Rol | Taak | Draait op |
| --- | --- | --- |
| Research Lead | scope, deelvragen, hypotheses, stopcriteria, budget; stuurt bij op tussenresultaten | hoofdmodel |
| Scouts + crawler-team | zoeken en gericht crawlen (zie [[modules/research-kennis]]) | CPU + router-model |
| Readers (parallel) | per bron de relevante passages | 1060 (klein model) |
| Extractor / data-omzetter | passages → getypeerde records (zie schema's), direct gevalideerd | 1060 + schema-validatie |
| Bronbeoordelaar | primair/secundair, actualiteit, onafhankelijkheid, betrouwbaarheid | hoofdmodel + regels |
| Analysts (1–3) | per deelvraag synthese, cross-document redeneren, berekeningen in sandbox | hoofdmodel |
| Fact-checker / verifier | elke kernclaim trianguleren met onafhankelijke bronnen | hoofdmodel |
| Skepticus / gap-finder | tegenbewijs, ontbrekend bewijs, welke zoekactie levert nu het meeste op (value-of-information) | hoofdmodel |
| Judge | is het genoeg en klopt het? anders terug naar Lead met gerichte opdracht | hoofdmodel / ander model |
| Writer | rapport met brongebonden claims, tabellen, bewijsoverzicht | hoofdmodel |
| Archivist | records + bevindingen naar geheugen/graph, conflicten → conflict-team, aanpak → procedural memory, data → datasets | core |

## Loop
1. **Plan** — Lead maakt deelvragen + hypotheses + stopcriteria in de Global Workspace.
2. **Vind** — Scouts/crawlers; Ranker kiest volgende bron.
3. **Lees + zet om** — Readers en Extractor streamen records direct naar de Archivist (meteen in geheugen, met herkomst).
4. **Analyseer** — Analysts per deelvraag, rekenen in sandbox, cross-document.
5. **Verifieer + zoek gaten** — Fact-checker trianguleert; Skepticus bepaalt de waardevolste volgende zoekactie.
6. **Oordeel** — Judge: genoeg bewijs per deelvraag? Nee → gerichte nieuwe ronde. Ja → schrijven.
7. **Lever op** — rapport, datatabellen, graph-updates, dataset-records.
8. **Leer** — welke bronnen/zoekroutes werkten → Ranker-training, skills, bronbetrouwbaarheid bijwerken.

## Datatypen (Extractor-schema's)
| Record | Velden |
| --- | --- |
| Claim | bewering, polariteit, bron, passage, datum, confidence, status (gestaafd/tegengesproken/onbevestigd) |
| Getal / meetwaarde | waarde, eenheid, wat, wanneer, waar, bron, onzekerheid |
| Entiteit | naam, type, aliassen, Wikidata-ID indien bekend |
| Relatie | subject, predicaat, object, geldig van/tot, bron |
| Event | wat, wie, waar, wanneer, gemeld/bevestigd, bronnen |
| Tabel | structuur behouden (kopregels, eenheden, voetnoten) |
| Citaat | letterlijke tekst (kort), spreker, bron, datum |
| Bron | URL/bestand, type, uitgever, datum, betrouwbaarheid, onafhankelijkheidsgroep |
Alle records: één ID, herkomst, versie → direct doorzoekbaar, koppelbaar in de graph, exporteerbaar (CSV/Parquet), bruikbaar als trainingsdata (Q&A-paren, evals).

## Modi
- **Quick lookup** — Scout + Reader + Writer, één ronde.
- **Standaard** — + Extractor, Analyst, Fact-checker, Judge.
- **Deep research** — volledig team, meerdere rondes, Skepticus actief, rapport + dataset.
Metacognitie kiest; Research-knop in de invoerbalk overrulet.

## Hardware-verdeling (één taak tegelijk op hoofd-GPU)
Crawlers op CPU en Readers/Extractor op de 1060 lopen **parallel** door terwijl het hoofdmodel analyseert — zo is de 5060 Ti nooit idle tijdens research.

## GUI
Research-pagina: live tabel met binnenkomende records, deelvragen met status, bronnenlijst met betrouwbaarheid, bewijsoverzicht, eindrapport + data-export.

## Verbindingen
Event log (alle stappen), Global Workspace (plan/hypotheses), geheugen + graph (records), conflict-team, neurale layers (Ranker, entity matcher, verifier), training (Q&A-paren, DPO uit Judge-oordelen), eval (research-set).
