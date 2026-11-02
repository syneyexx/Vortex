---
wave: W07
status: af  # gate groen in tests (metacog-gate + compactie-gate); echte modellen op de PC
modules: [cognitie, context]
---
# W07 — Cognitie en context
Zie [[modules/cognitie]], [[modules/context]].

- [x] `W07.01` Cognitieve cyclus (8 fasen) als expliciete pipeline met events per fase — `vortex_cognition::cycle` (8 fasen, event `cognition.phase.completed` per fase); chat loopt volledig door `ChatEngine::run` (server/cognitive.rs)
- [x] `W07.02` Global Workspace (datamodel + API; alle modules lezen/schrijven) — `Workspace` (doel, plan, herinneringen, hypotheses, notities, strategie, bijsturing); elke wijziging = event; API `/api/runs/:id/workspace`
- [x] `W07.03` Doelstapel + goal decomposition + dead-task elimination — `push_goal`/`drop_goal` (snoeit subdoelen + afhankelijken) / `ready_goals`; plan-stappen worden doelen
- [x] `W07.04` Metacognitie-controller: snel/diep, model, team, budget, stoppen, vragen; dynamic strategy switching; selective prediction (heuristische drempels) — `metacog::decide` (pad, plan, effort, budget, voorspelling antwoord/meer bewijs/vragen/geen conclusie) + `switch_on` bij stall; beslissing in DecisionRecord
- [x] `W07.05` Planning: ReAct, plan-and-execute, hiërarchisch; reflexion/self-critique — `planning::{make_plan, execute_plan, critique, react}`; ReAct krijgt echte tools in W08
- [x] `W07.06` Router (heuristiek + LM-label; net volgt W11) — heuristiek + LM-label via router-model (alleen bij zekerheid < 0,65, max 1,5 s); geleerd net volgt W11
- [x] `W07.07` Salience/valentie-tags — `salience::tag` (urgent, nieuw, tegenstrijdig, valentie) → event
- [x] `W07.08` Leeragenda: kennisgaten registreren — `Agenda` (tabel `learning_agenda`, migratie 0005): gaten + betwiste feiten; GUI dashboard; zelf onderzoeken in W15
- [x] `W07.09` Context Assembler: antwoordbudget, tool discovery, MMR + tegenbewijs, verliesbewuste compactie, selectieve contextbewerking, hiërarchische gesprekssamenvatting, geïsoleerde agentcontexten, filteren met code, prompt caching — `vortex_context::assembler` (budget per sectie, tool discovery = naam+beschrijving, MMR + tegenbewijs uit W05, compactie bewaart exacte gegevens, gists + sessiesamenvatting, `assemble_for_role` = geïsoleerd, `filter_tool_output`, stabiele prefix + `cache_prompt`)
- [x] `W07.10` Interruptible/resumable + gebruiker kan bijsturen tijdens run — checkpoints per planstap + stop; bijsturen tijdens streamen (`POST /api/runs/:id/steer`, Enter in de invoerbalk) → antwoord gaat verder met de nieuwe instructie
- [x] `W07.11` Quick/Deep/Research/Adaptive aangesloten — modi sturen pad/effort/budget; Research gebruikt het diepe pad tot W10 de researchpipeline levert
- [x] `W07.12` User model: voorkeuren leren over tijd, stijl- en toonaanpassing, lange-termijndoelen, projectvoorkeuren, gepersonaliseerde retrieval — `UserProfile` uit gebruikersgeheugen + feedback → stijlblok in elke prompt; persoonlijke vragen halen user/project-geheugen voorrang; GUI "Over jou"

## Gate
Eval: Adaptive kiest het juiste pad; contextcompactie verliest geen exact-te-bewaren gegevens (test).
