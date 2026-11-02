---
type: architectuur
updated: 2026-10-31
---
# One brain — hoe alles verbonden is (regel 1)

Vier gedeelde ruggengraten. **Elke module moet op alle vier aangesloten zijn**, anders is hij niet af.

## 1. Event log (één bron van wat er gebeurde)
Elke actie is een append-only event (bericht, plan, toolcall, retrieval, debatronde, oordeel, feedback, model-/promptversie). Event + statuswijziging in één SQLite-transactie (outbox).
Voedt: tracing, replay, GUI-stream, geheugen-consolidatie, trainingsdata voor LoRA én neurale layers, eval.

## 2. Global Workspace (het denkende nu)
Per taak één gedeeld schoolbord: doel, doelstapel, plan-state, opgehaalde herinneringen, hypotheses, open vragen, budget, onzekerheden. Alle agents lezen/schrijven hier; Context Assembler maakt er per stap de prompt van.

## 3. Geheugen + knowledge graph (wat Vortex weet)
Eén ID per herinnering; vector, keyword, graph en temporeel zijn afgeleide indexen. Alles wat binnenkomt (chat, tools, research, feeds, datasets, skills-resultaten) gaat via dezelfde schrijfpipeline. Conflicten → conflict-team.

## 4. Metacognitie + neurale layers (hoe Vortex beslist)
Router-net, importance scorer, reranker, verifier, calibrator nemen beslissingen in elke module; ze leren van het event log en worden via de eval-suite gepromoveerd.

## Dataflow van één vraag
1. Waarnemen → event + herkomstlabel
2. Router-net: taaktype, moeilijkheid → metacognitie kiest snel/diep, model, team, budget
3. Geheugen ophalen (hybrid + rerank + MMR + tegenbewijs) → workspace
4. Debatteam werkt in de workspace; tools via contracten; fact-checker lokaal + web (indien online)
5. Judge vertrouwt → Synthesizer antwoordt; anders eerlijk onzeker / vraag
6. Reflectie: kennisgaten → leeragenda; conflicten → conflict-team
7. Consolidatie: geheugen bijwerken, skill-voorstel, traces → training
8. Idle: dreaming, neurale layers trainen, leeragenda onderzoeken, LoRA-vliegwiel

## Verbindingsmatrix (checklist bij elke module)
| Module | Event log | Workspace | Geheugen | Metacog/neural | GUI |
| --- | --- | --- | --- | --- | --- |
| Runtime core | schrijft alles | beheert levensduur | — | ontvangt budget | status |
| Modelbeheer | lifecycle-events | — | — | router kiest model | Models-pagina |
| Geheugen | schrijf/lees-events | levert herinneringen | — | importance, reranker | Memory-pagina |
| Cognitie | elke fase | eigenaar | leest/schrijft | eigenaar | Agent-paneel |
| Debatteam | rondes + oordelen | werkt erin | fact-check | verifier, calibrator | Agent-paneel |
| Tools/skills | toolcalls | resultaten | tool → geheugen | skill selector | tool-kaarten |
| Research/kennis | ingest-events | onderwerp-context | schrijft kennis | entity matcher | Research-pagina |
| Training | leest log | — | — | traint layers + LoRA | dashboard |
| Autonomie | gepland werk | — | leeragenda | prioriteit | briefing |
