---
type: module
crate: vortex-cognition
waves: [W06, W11, W17]
updated: 2026-10-31
---
# Cognitieve architectuur

Het deel van het brein dat in het framework zelf zit.

## Cognitieve cyclus (per taak/stap)
1. **Waarnemen** — normaliseren, herkomstlabel
2. **Aandacht** — salience: belangrijk, urgent, nieuw, tegenstrijdig
3. **Herinneren** — passend + proactief geheugen
4. **Overwegen** — snel pad (System 1) of debatteam (System 2); planning (ReAct, plan-and-execute, hiërarchisch, Tree/Graph-of-Thoughts waar nuttig)
5. **Handelen** — getypeerde acties met pre/postchecks
6. **Verifiëren** — fact-checker, tests, Judge; lokaal altijd, web indien online
7. **Reflecteren** — wat werkte; kennisgaten → leeragenda
8. **Consolideren** — geheugen, skill-voorstellen, traces

## Onderdelen
- **Global Workspace** — doel, plan-state, herinneringen, hypotheses, open vragen, budget, onzekerheid. Ruggengraat [[11-ONE-BRAIN]].
- **Doelstapel** — hoofd/subdoelen, afhankelijkheden; doelwijziging snoeit plan (dead-task elimination).
- **Metacognitie-controller** — snel/diep, model, team, budget, stoppen, gebruiker vragen; dynamic strategy switching; stall-signalen; selective prediction (antwoorden / meer bewijs / vragen / geen conclusie).
- **Zelfmodel** — sterktes/zwaktes per taaktype uit eval (W11).
- **Wereldmodel** — temporal KG + causale hypotheses met vertrouwen; voorspellingen doen en later toetsen (W13).
- **Graph-redeneren** — afleidingen over relaties, consistentie, regels zonder LM (W11).
- **Leeragenda/nieuwsgierigheid** — kennisgaten, betwiste feiten, zwakke taaktypen; registreren W6, zelf onderzoeken in idle W14.
- **Salience/valentie-tags.**
- **Value-of-information planning** + **cost-based plan selection** (W16).
- **User model** — voorkeuren, stijl, lange-termijndoelen, projectvoorkeuren (via geheugen).
- **Interruptible/resumable** via checkpoints; gebruiker kan bijsturen.

## Snel vs langzaam
System 1: één agent, klein budget. System 2: volledige cyclus + team. Override: Quick / Deep / Research / Adaptive (standaard).

## Frontier (W17)
Neural memory module, continual learning zonder catastrophic forgetting, causaal/counterfactual redeneren, wereldmodel-simulaties.
