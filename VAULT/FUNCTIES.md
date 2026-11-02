---
type: overzicht
updated: 2026-11-02
---
# Functies — alles wat Vortex kan (per domein)

Het overzicht van **wat** Vortex kan. De afvinklijst (**hoe/wanneer bouwen**) staat in [[TODO]] (gegenereerd uit `waves/`). Tussen haakjes de wave(s); details in de module.

## 1. Chat en interactie — [[modules/gui]], [[modules/context]]
- Chatten met streaming, tok/s, stopknop die echt stopt (W03)
- Modi: **Quick / Deep / Research / Adaptive** (standaard Adaptive, metacognitie kiest) (W03, W06)
- Wisselt per bericht vanzelf tussen praten, code, research, analyse, rekenen (W06, W08)
- Markdown, codeblokken (highlighting, kopiëren, diff, uitvoeren in sandbox), tabellen, LaTeX, Mermaid, artefacten (W03, W08)
- Sessies opslaan/herstellen, branching/forking, geschiedenis, project-werkruimtes (W03, W04)
- System prompt + rol-prompts live bewerken, versies + terugzetten (W03)
- Feedbackknoppen goed/fout/correctie → trainingsdata (W05)
- Spraak in/uit, voice-modus (W15) · telefoon via Tailscale (W14)

## 2. Denken en redeneren — [[modules/cognitie]], [[modules/redeneren-debat]]
- Cognitieve cyclus in 8 fasen: waarnemen → aandacht → herinneren → overwegen → handelen → verifiëren → reflecteren → consolideren (W06)
- **Global Workspace**: gedeeld schoolbord per taak (W06)
- Doelstapel, goal decomposition, dead-task elimination (W06)
- Metacognitie: snel/diep, model, team, budget, stoppen, vragen; selective prediction (W06)
- Planning: ReAct, plan-and-execute, hiërarchisch, Tree/Graph-of-Thoughts (W06)
- **Debatteam**: Planner, Solvers (2–3), Fact-checker, Critic, Specialist, Judge, Synthesizer (W08)
- **Judge-gate**: antwoord pas tonen als de Judge het vertrouwt; anders eerlijk onzeker of een vraag (W08)
- **Conflict-team**: lost botsende feiten direct op en corrigeert geheugen (W08)
- Offline-first fact-check; automatisch webcheck als er verbinding is (W08)
- Zelfmodel (sterk/zwak per taaktype), wereldmodel met voorspellingen die later getoetst worden (W11, W13)
- Graph-redeneren zonder LM (W11) · causaal/counterfactual redeneren (W17)
- User model: voorkeuren, stijl, doelen (W06)

## 3. Geheugen — [[modules/geheugen]]
- 9 typen: working, short-term, long-term, episodic, semantic (temporal KG), user/project, world, procedural, meta-memory (W04, W11, W13)
- Schrijfpipeline: importance-policy, structuur-bewust chunken, contextbeschrijving, embeddings, entiteiten + claims, graph, dedup, conflictdetectie (W04)
- Hybride ophalen: vector + FTS5/BM25 + graph multi-hop + temporeel → rerank → MMR + tegenbewijs (W04)
- "Vergeet dit" met deletion propagation door alle indexen (W04)
- Dreaming in idle: consolideren, samenvatten, decay, contradiction resolution (W11)
- Schaal naar **3–4 TB**: index op SSD, inhoud op HDD (tiering) (W11)
- Export/import, backup → HDD, restore (W04, W14)

## 4. Modellen en inference — [[modules/inference-modelbeheer]], [[05-MODELLEN]]
- Model-registry met hot-reload; wisselen = één instelling (W02)
- Twee GPU's: 5060 Ti wisselt (hoofdmodel/specialisten), 1060 vast team (embed, rerank, router) (W00, W02)
- Lifecycle (7 fasen), readiness-checks, draining, KV-cache save/restore (W02)
- Toolcall-parser per model, structured output / grammar (W02)
- MoE expert-offload, KV-quant, quant-shootout, speculative decoding bij gemeten winst (W02, W16)
- Eigen llama.cpp-bindings en GPU-kernels alleen bij gemeten hotspot (W16)

## 5. Tools en skills — [[modules/tools-skills]]
- Toolcontracten (schema, bijwerkingen, timeout, idempotent, resources) (W07)
- Bestanden, web search (SearXNG), pagina lezen, code-interpreter (Python/shell) in sandbox, SQLite, API-calling, geheugen-tools (W07)
- **MCP-client** (stdio + HTTP) (W07)
- **Skills**: importeren/exporteren Claude Skills ⇄ Cursor rules ⇄ AGENTS.md; zelf skills schrijven uit geslaagde traces (W07, W11)
- Documenten, audio, beeld (W15) · browser automation, git, computer use, repo-begrip, programmatic tool calling (W16)
- Vortex schrijft en registreert eigen tools (W17)

## 6. Research en kennis — [[modules/research-kennis]], [[modules/research-team]]
- **Research-team** (11 rollen): van vraag naar geverifieerde kennis + getypeerde data (W09)
- **Eigen webcrawler-team**: Scout, Crawler, Reader, Ranker, Curator; focused crawling (W09, W13)
- Ebooks (EPUB/PDF), docs, code, lokale mappen doorzoekbaar met exacte bronverwijzing (W09)
- Rapporten met brongebonden claims, export CSV/Parquet (W09)
- **Wereldmonitor**: GDELT, RSS, Wikipedia/Wikidata, ReliefWeb, USGS, NASA, World Bank, Eurostat, CBS, arXiv, HN (W13)
- **Grote spelers per sector** bestuderen en ervan leren (W13)
- **Hugging Face-datasets** inladen → kennis / training / eval (W13)
- Visie, OCR, tabellen, visuele documentretrieval (W15)

## 7. Leren en trainen ("50% van het brein") — [[modules/neurale-layers]], [[modules/training]]
- Alles loggen vanaf dag 1 (W01)
- **Eigen neurale layers** (ONNX): router, importance, reranker, usefulness, entity matcher, verifier, calibrator, decay, skill selector, salience (W10, W12)
- **LoRA-vliegwiel**: distillatie/SFT, DPO, QLoRA op Qwen3.5-9B → GGUF (W12)
- Poortwachter: eval + schaduwrun, slechter = automatisch terug (W10, W12)
- Neural memory, continual learning (W17)

## 8. Autonomie — [[modules/autonomie]]
- Idle-scheduler: zwaar werk alleen als de PC vrij is; GPU wijkt direct voor de gebruiker (W14)
- Leeragenda zelf uitvoeren, inhaal-modus na opstarten (W14)
- Taakqueue, planning/triggers, proactieve suggesties, dagelijkse briefing (W14)
- Lokale OpenAI-compatibele API (Cursor/eigen projecten) (W14)

## 9. Fundament, betrouwbaarheid, snelheid — [[modules/runtime-core]], [[modules/orkestrator-performance]], [[modules/observability-eval]]
- Run state machine, cancellation door de hele keten, checkpoints, crash-herstel (W01)
- **Dirigent**: P0–P3, admission control, preemption, model-affiniteit, pipelining, drukmeters (W01)
- Event log (append-only, outbox) = ruggengraat van alles (W01)
- Tracing, eval-suite, Context Inspector, session replay, soak-test 24 u (W05)
- Ablations, fault injection, A/B, semantische cache (W16)

## 10. Geparkeerd — [[GEPARKEERD]]
- Ethics & security layer (W18) — trigger: **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**
- Trading (W19) — trigger: **"TRADING NU MAKEN"**
