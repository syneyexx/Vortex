---
type: overzicht
updated: 2026-11-02
---
# Functies — alles wat Vortex kan (per domein)

> **Stand 2026-11-02:** alles hieronder (W00–W18) is gebouwd; wat nog op de PC geverifieerd moet worden staat in [[PC-VERIFICATIE]], de stand in [[OVERDRACHT]]. Geschrapt op verzoek: mobiele weergave, quantum-tool (Qiskit/IBM). Niet gebouwd: W19, W20 (alleen op commando).

Het overzicht van **wat** Vortex kan. De afvinklijst (**hoe/wanneer bouwen**) staat in [[TODO]] (gegenereerd uit `waves/`). Tussen haakjes de wave(s); details in de module.

## 1. Chat en interactie — [[modules/gui]], [[modules/context]]
- Chatten met streaming, tok/s, stopknop die echt stopt (W03)
- Modi: **Quick / Deep / Research / Adaptive** (standaard Adaptive, metacognitie kiest) (W03, W07)
- Wisselt per bericht vanzelf tussen praten, code, research, analyse, rekenen (W07, W09)
- Markdown, codeblokken (highlighting, kopiëren, diff, uitvoeren in sandbox), tabellen, LaTeX, Mermaid, artefacten (W03, W09)
- Sessies opslaan/herstellen, branching/forking, geschiedenis, project-werkruimtes (W03, W05)
- System prompt + rol-prompts live bewerken, versies + terugzetten (W03)
- Feedbackknoppen goed/fout/correctie → trainingsdata (W06)
- Spraak in/uit, voice-modus (W16) · telefoon via Tailscale, gewone GUI zonder aparte mobiele weergave (W15)

## 2. Denken en redeneren — [[modules/cognitie]], [[modules/redeneren-debat]]
- Cognitieve cyclus in 8 fasen: waarnemen → aandacht → herinneren → overwegen → handelen → verifiëren → reflecteren → consolideren (W07)
- **Global Workspace**: gedeeld schoolbord per taak (W07)
- Doelstapel, goal decomposition, dead-task elimination (W07)
- Metacognitie: snel/diep, model, team, budget, stoppen, vragen; selective prediction (W07)
- Planning: ReAct, plan-and-execute, hiërarchisch, Tree/Graph-of-Thoughts (W07)
- **Debatteam**: Planner, Solvers (2–3), Fact-checker, Critic, Specialist, Judge, Synthesizer (W09)
- **Judge-gate**: antwoord pas tonen als de Judge het vertrouwt; anders eerlijk onzeker of een vraag (W09)
- **Conflict-team**: lost botsende feiten direct op en corrigeert geheugen (W09)
- Offline-first fact-check; automatisch webcheck als er verbinding is (W09)
- Zelfmodel (sterk/zwak per taaktype), wereldmodel met voorspellingen die later getoetst worden (W12, W14)
- Graph-redeneren zonder LM (W12) · causaal/counterfactual redeneren (W18)
- User model: voorkeuren, stijl, doelen (W07)

## 3. Geheugen — [[modules/geheugen]]
- 9 typen: working, short-term, long-term, episodic, semantic (temporal KG), user/project, world, procedural, meta-memory (W05, W12, W14)
- Schrijfpipeline: importance-policy, structuur-bewust chunken, contextbeschrijving, embeddings, entiteiten + claims, graph, dedup, conflictdetectie (W05)
- Hybride ophalen: vector + FTS5/BM25 + graph multi-hop + temporeel → rerank → MMR + tegenbewijs (W05)
- "Vergeet dit" met deletion propagation door alle indexen (W05)
- Dreaming in idle: consolideren, samenvatten, decay, contradiction resolution (W12)
- Schaal naar **3–4 TB**: index op SSD, inhoud op HDD (tiering) (W12)
- Export/import, backup → HDD, restore (W05, W15)

## 4. Modellen en inference — [[modules/inference-modelbeheer]], [[05-MODELLEN]]
- Model-registry met hot-reload; wisselen = één instelling (W02)
- Twee GPU's: 5060 Ti wisselt (hoofdmodel/specialisten), 1060 vast team (embed, rerank, router) (W00, W02)
- Lifecycle (7 fasen), readiness-checks, draining, KV-cache save/restore (W02)
- Toolcall-parser per model, structured output / grammar (W02)
- MoE expert-offload, KV-quant, quant-shootout, speculative decoding bij gemeten winst (W02, W17)
- Eigen llama.cpp-bindings en GPU-kernels alleen bij gemeten hotspot (W17)

## 5. Tools en skills — [[modules/tools-skills]]
- Toolcontracten (schema, bijwerkingen, timeout, idempotent, resources) (W08)
- Bestanden, web search (SearXNG), pagina lezen, code-interpreter (Python/shell) in sandbox, SQLite, API-calling, geheugen-tools (W08)
- **MCP-client** (stdio + HTTP) (W08)
- **Skills**: importeren/exporteren Claude Skills ⇄ Cursor rules ⇄ AGENTS.md; zelf skills schrijven uit geslaagde traces (W08, W12)
- Documenten, audio, beeld (W16) · browser automation, git, computer use, repo-begrip, programmatic tool calling (W17)
- Vortex schrijft en registreert eigen tools (W18)

## 6. Research en kennis — [[modules/research-kennis]], [[modules/research-team]]
- **Research-team** (11 rollen): van vraag naar geverifieerde kennis + getypeerde data (W10)
- **Eigen webcrawler-team**: Scout, Crawler, Reader, Ranker, Curator; focused crawling (W10, W14)
- Ebooks (EPUB/PDF), docs, code, lokale mappen doorzoekbaar met exacte bronverwijzing (W10)
- Rapporten met brongebonden claims, export CSV/Parquet (W10)
- **Wereldmonitor**: GDELT, RSS, Wikipedia/Wikidata, ReliefWeb, USGS, NASA, World Bank, Eurostat, CBS, arXiv, HN (W14)
- **Grote spelers per sector** bestuderen en ervan leren (W14)
- **Hugging Face-datasets** inladen → kennis / training / eval (W14)
- Visie, OCR, tabellen, visuele documentretrieval (W16)

## 7. Leren en trainen ("50% van het brein") — [[modules/neurale-layers]], [[modules/training]]
- Alles loggen vanaf dag 1 (W01)
- **Eigen neurale layers** (ONNX): router, importance, reranker, usefulness, entity matcher, verifier, calibrator, decay, skill selector, salience (W11, W13)
- **LoRA-vliegwiel**: distillatie/SFT, DPO, QLoRA op Qwen3.5-9B → GGUF (W13)
- Poortwachter: eval + schaduwrun, slechter = automatisch terug (W11, W13)
- Neural memory, continual learning (W18)

## 8. Autonomie — [[modules/autonomie]]
- Idle-scheduler: zwaar werk alleen als de PC vrij is; GPU wijkt direct voor de gebruiker (W15)
- Leeragenda zelf uitvoeren, inhaal-modus na opstarten (W15)
- Taakqueue, planning/triggers, proactieve suggesties, dagelijkse briefing (W15)
- Lokale OpenAI-compatibele API (Cursor/eigen projecten) (W15)

## 9. Fundament, betrouwbaarheid, snelheid — [[modules/runtime-core]], [[modules/orkestrator-performance]], [[modules/observability-eval]]
- Run state machine, cancellation door de hele keten, checkpoints, crash-herstel (W01)
- **Dirigent**: P0–P3, admission control, preemption, model-affiniteit, pipelining, drukmeters (W04)
- Event log (append-only, outbox) = ruggengraat van alles (W01)
- Tracing, eval-suite, Context Inspector, session replay, soak-test 24 u (W06)
- Ablations, fault injection, A/B, semantische cache (W17)

## 10. Geparkeerd — [[GEPARKEERD]]
- Ethics & security layer (W19) — trigger: **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**
- Trading (W20) — trigger: **"TRADING NU MAKEN"**
