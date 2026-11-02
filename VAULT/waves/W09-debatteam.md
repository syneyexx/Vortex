---
wave: W09
status: af  # code + tests groen; gate-vergelijking team vs single-agent met echt model op de PC: `vortex eval --suite moeilijk --mode quick` en `--mode deep`
modules: [redeneren-debat]
---
# W09 — Debatteam
Zie [[modules/redeneren-debat]].

- [x] `W09.01` Rollen met prompts in `config/roles/` (bewerkbaar in Instellingen) — 10 rollen in `config/roles/*.md` (frontmatter temperature/model_role); standaarden ingebouwd + bij start weggeschreven; bewerkbaar in Instellingen (prompts & rollen, met versies)
- [x] `W09.02` Dynamische teamsamenstelling door metacognitie — `team::compose(route, strategy, modus)`: eenvoudig → geen team; moeilijk → Planner/Solvers (2–3)/Fact-checker/Critic/Specialist/Judge, rondes 2–3
- [x] `W09.03` Solvers met diversiteit (hoek, temperatuur, evt. model) — 4 invalshoeken, temperatuur per solver, optioneel extra modellen (`DebateDeps::extra`)
- [x] `W09.04` Fact-checker: claims splitsen (1060), bewijs lokaal; connectiviteitscheck → webcheck online, label offline — claims splitsen op het extract-model (1060) met terugval; bewijs uit geheugen (altijd) + web_search als online (`Connectivity`, 60 s cache); offline → scope "lokaal" en label "lokaal geverifieerd"
- [x] `W09.05` Critic, Specialist (code-reviewer draait tests), verifier-guided candidate selection — Critic per kandidaat; Specialist code (Python-blokken echt draaien) en reken (sandbox rekent na); verifier-score (objectieve checks > critic > claims) kiest de kandidaat
- [x] `W09.06` Judge-gate: antwoord pas na vertrouwen; max rondes; anders eerlijk onzeker/vraag — Judge (JSON verdict/confidence); accept ≥ 0,6 = vertrouwd; revise → herziening met feedback; na max rondes → Synthesizer moet eerlijk onzeker zijn / vraag stellen. Antwoord streamt pas ná het oordeel (test)
- [x] `W09.07` Synthesizer in gebruikersstijl — `synthesizer_guidance`: rolprompt + goedgekeurd concept + tegengesproken claims weglaten + gebruikersprofiel via de assembler
- [x] `W09.08` Bewijsoverzicht per conclusie — event `agent.evidence.compiled` (bewering, label, bron, datum, scope) → bewijskaart onder het antwoord
- [x] `W09.09` Conflict-team (2 onderzoekers, bronbeoordelaar, judge, memory writer), voorgrond/achtergrond, geldigheidsperiodes, "betwist", rollback — `conflict::resolve`: regels (gebruiker wint; nieuwere eigen uitspraak = periodes) of team (2 onderzoekers ∥, bronbeoordelaar + regels, Judge); één webpagina wint niet automatisch → betwist; `apply_resolution` + `rollback_conflict` (migratie 0007); voorgrond in de chat, achtergrond-worker (P3), knoppen in het conflictregister
- [x] `W09.10` Agent collaboration-paneel + Verdict-balk live; voortgang gescheiden van antwoord — `agent.team.composed` / `agent.role.{started,completed}` / `agent.judge.verdict` → paneel toont het echte team; Verdict-balk toont het Judge-oordeel; voortgang los van het antwoord
- [x] `W09.11` Parallel onderzoek / subagents met geïsoleerde contexten, overdracht via verwijzingen — Solvers parallel (`join_all`) met geïsoleerde contexten (`assemble_for_role`); kandidaten als hypotheses in de Global Workspace, overdracht via kandidaat-ID

## Gate
Eval: team scoort aantoonbaar hoger dan single-agent op moeilijke set; offline modus foutloos; conflicten worden correct opgelost.
