---
wave: W18
status: af  # alles achter vlaggen (standaard uit) + meting; quantum-tool geschrapt op verzoek gebruiker
modules: [cognitie, neurale-layers, tools-skills]
---
# W18 — Frontier

- [x] `W18.01` Neural memory module (differentieerbaar associatief geheugen) — modern Hopfield met geleerde query-projectie (`vortex_memory::assoc`), online getraind op (vraag, relevante herinnering); leert associaties waar cosinus faalt (test); als reranker achter vlag `frontier.assoc`; experiment `assoc` meet MRR met/zonder op de retrieval-set
- [x] `W18.02` Continual learning zonder catastrophic forgetting (adapters, replay, regularisatie) — layers: replay van de vorige dataset, L2-SP + fine-tunen vanaf de vorige versie, vergeet-check in de poortwachter (> 2 pt achteruit op de oude testset = afgewezen); meting: naïef vergeet 35 pt, replay 7 pt, replay+L2-SP 9 pt → aparte vlaggen; LM: adapters (W13) + gate
- [x] `W18.03` Causaal/counterfactual redeneren en tracking — `vortex_cognition::causal`: graaf uit de hypotheses (noisy-OR), effecten van do(X), counterfactual ("wat als X niet"), causale claims uit antwoorden → hypotheses (vlag `causal_tracking`); `/api/causal/{effects,counterfactual}`
- [x] `W18.04` Wereldmodel-simulaties — Monte Carlo over de causale graaf met ingrepen (aan) en blokkades (uit): frequentie per gevolg + meest voorkomende scenario's; `/api/world/simulate`
- [x] `W18.05` Vortex schrijft, test en registreert eigen tools — `create_tool`: Python `run(args)` + schema + ≥ 2 tests in de sandbox; alleen bij groen bewaard (`config/tools/generated/`) en geregistreerd, versies, weer geladen na herstart; geen ingebouwde namen, alleen Local
- [x] `W18.06` Self-improving prompts en tools — kandidaat-systeemprompts uit eigen fouten (mislukte evals, correcties) → A/B per kandidaat → alleen een aantoonbare winnaar wordt een suggestie (toepassen = nieuwe configversie); zelfgeschreven tools met fouten worden herschreven en opnieuw getest (taak `improve_tools`)
- [x] `W18.07` Multi-agent gedeeld + privé geheugen — privé per rol (`agent:<rol>:<run>`), gedeeld per team (`team:<run>`), gestaafde beweringen pas algemene kennis na Judge-akkoord (`vortex_agents::agentmem`, vlag `agent_memory`)
- [x] `W18.08` Quantum-tool: Qiskit, circuits lokaal simuleren (~30 qubits haalbaar) + IBM Quantum Open Plan (10 min per 28 dagen) voor echte QPU-runs in research — **geschrapt op verzoek van de gebruiker (2026-11-02: "heeft geen nut")**; gebouwd en weer verwijderd
- (Quantum-experimenten 1 en 3 naar voren gehaald op verzoek gebruiker: zie W12 en W17)

## Gate
Elk onderdeel levert meetbare eval-winst, anders niet ingeschakeld.
