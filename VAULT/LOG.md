---
type: log
---
# Sessielog
Nieuwste onderaan. Per sessie 3–5 regels: wat gedaan, wat open, volgende stap.

## 2026-10-31 (ontwerp-chat — geen code)
- Volledig ontwerp besproken: talen, geheugen, cognitie, neurale layers, debatteam, tools/skills, research, wereldmonitor, datasets, training, GUI, modellen, opslag.
- Input van Grok en ChatGPT Astra gefilterd en verwerkt (originelen in `bronnen/`).
- Werkregels 1–6 vastgelegd; VAULT aangemaakt; bouwplan in 20 waves (W00–W19).
- Later toegevoegd: eigen webcrawler-team, research-team ([[modules/research-team]]), Dirigent + performance-plan ([[modules/orkestrator-performance]]), CPU = Ryzen 9 5950X.
- Eerder ook een bouwplan-document in Claude Docs gemaakt (oudere v1-indeling); **deze VAULT is leidend**.
- VAULT op de PC gezet in `Desktop\VortexFramework\VAULT\`; `CLAUDE.md` in de projectroot wijst nieuwe chats hierheen.
- **Volgende stap:** nieuwe chat → [[waves/W00-fundament]] starten (eerst de checks: SSD, nvidia-smi, llama.cpp-build, laadtijden).

## 2026-11-01 (ontwerp, geen code)
- Onderzocht: eigen taal (nee), GPU-kernels (alleen bij gemeten hotspot), CUDA-toolchain sm_61 + sm_120, modelcompressie (MoE-cache, NVFP4, KV-quant, 1060 int8), quantum (tools + experimenten), tensor-compressie (nooit op redeneermodel).
- Een experimenteel neven-idee is afgesplitst als volledig apart project; hoort niet bij Vortex.
- **Volgende stap (Vortex):** [[waves/W00-fundament]].

## 2026-11-02 (VAULT naar GitHub, geen code)
- VAULT + `CLAUDE.md` uit `VortexFramework.zip` in repo `syneyexx/Vortex` gezet.
- Toegevoegd: [[FUNCTIES]] (alles op een rij per domein), [[TODO]] (230 items met ID, gegenereerd), [[12-NAVIGATIE]], [[20-CODEMAP]], [[13-CONVENTIES]], [[14-WOORDENLIJST]], [[OPEN-VRAGEN]], `templates/`, `tools/vault/vault.py`.
- Alle wave-items kregen een ID (`Wnn.mm`); dubbel W09-item verwijderd.
- Open: voorstellen in [[OPEN-VRAGEN]] bevestigen (vortex-types-crate, W01 splitsen, Judge-gate vs TTFT).
- **Volgende stap (19:00, op de PC):** repo klonen, [[OPEN-VRAGEN]] "Vanavond eerst" doorlopen, dan [[waves/W00-fundament]].

## 2026-11-02 (open vragen beantwoord, geen code)
- 13 open vragen beantwoord; besluiten in [[03-BESLUITEN]], overzicht in [[OPEN-VRAGEN]].
- Waves herzien: W01 gesplitst, nieuwe [[waves/W04-dirigent-performance]], oude W04–W19 → W05–W20. W00 uitgebreid (WSL2-test, `proto/`, `justfile`, `vortex` CLI).
- Nieuw: [[15-CONTRACTEN]]. Hardware: SSD = NVMe 250 GB.
- Open: wat "testen pas als laatst" precies betekent; PC-details (moederbord, voeding, Windows, HDD's).
- **Volgende stap (19:00):** repo klonen naar `C:\dev\Vortex`, dan [[waves/W00-fundament]] checks.

## 2026-11-02 (eerste code — Claude Code op het web, zonder GPU)
- **W01 runtime kern gebouwd** (`core/vortex-core`): SQLite + migraties, event log (append-only, outbox, dispatcher), state machine, structured concurrency, stopketen (3 statussen), budgetten/deadlines, begrensde wachtrijen, resource-register, checkpoints + effecten, config hot-reload met versies, secrets broker, procesbewaking, GPU-slot-scheduler, tracing, crash-rapport. 57 unit- + 8 integratietests groen; **W01-gate (kill -9 → exact herstel + hervatten) groen op Linux**.
- **W00 software-deel:** Rust-workspace (13 crates + richtingscontrole), `proto/` + codegen Rust/Python/TS, Python-worker (gRPC Health/Cancel), app-skelet (Vite + Tauri-config + iconen), `config/`, `justfile`, `vortex` CLI (`serve`/`status`/`runs`), CARD.md per crate. `just check` en `just test` groen.
- Open (alleen op de PC): hardware-checks, CUDA/llama.cpp, modellen downloaden + metingen, Tauri-build, Windows-paden van W01.14/W01.15, `models-up.ps1` testen.
- **Volgende stap:** op de PC W00 afmaken (checks + modellen + metingen), dan W02 — in de cloud kan ik verder met de delen van W02 die geen GPU nodig hebben.

## 2026-11-02 (W02 code-deel)
- `vortex-models` gebouwd: registry + validatie, Backend-trait, OpenAI-adapter (chat/stream/embeddings/rerank, json_schema/GBNF, reasoning_effort), capability negotiation, toolcall-parsers (`openai-native/v1`, `hermes-xml/v1`), UTF-8/stop/tool-args-streaming, metingen, lifecycle (7 fasen, draining, wisselen met KV save/restore, idle → thuismodel, voorverwarmen).
- `vortex-core`: langlevende processen (`spawn_service`/`ServiceHandle`).
- 32 tests groen (nep-server in axum + Python-test-dubbel van llama-server). Bug gevonden en opgelost: draining hield de lock vast, waardoor nieuwe verzoeken wachtten i.p.v. geweigerd te worden.
- Open (PC): echte llama-server-protocol per model, wisseltijden, piek-VRAM, MoE-benchmark, quant-shootout, 1060 int8, speculative, specialisten (W02.10–13, W02.16).

## 2026-11-02 (afspraak testen)
- Regel 7 vastgelegd: kleine tests tussendoor altijd; grote tests (24 u soak, weekruns, volledige eval/performance-suites, ablations, fault-injection, TB-simulatie) alleen op commando. Gemarkeerd met 🔒 in W06, W12, W14, W15, W17 en in [[GEPARKEERD]] §1b.
- GUI-referentie (`design/reference-chat.png`) naar gebruiker gestuurd.
- Gestopt na W02-code: volgende gate (W00) vraagt de PC.

## 2026-11-02 (W03 GUI + chat)
- Server: sessies/berichten (migratie 0002), chat-engine (run P0 → GPU-slot → stream → deltas in event log), WebSocket-eventstream met hervatten, REST voor sessies/stop/runs/modellen/status/prompts (versies + terugzetten). 7 integratietests.
- GUI 1-op-1 opgebouwd: tokens, topnav/subtabs, WebGL-vortex, agent- en geheugenpaneel (eerlijk leeg tot W05/W09), chat met markdown/code/tabellen/LaTeX/Mermaid, invoerbalk met echte stop, taakbalk, statusbalk, instellingen, Lopend werk, geschiedenis/fork, notificaties. Reducer-tests (4).
- Visuele vergelijking met Playwright vond 3 fouten (o.a. **herladen tijdens streamen verloor tekst**) — opgelost en getest.
- Open: gate (chat met echt gpt-oss-20b) + GPU-weergave + modelwissel via ModelServer op de PC; pauzeren (W04); artefacten (W08).

## 2026-11-02 (W04 Dirigent + performance)
- `dirigent`: P0–P3, admission control per middel, preemption op veilige punten (`Lease::should_yield/yield_now`), priority inheritance, model-affiniteit, bulkheads, backpressure, condities (idle/game/druk), kostenmodel, pauzeren. Bug gevonden: bulkhead-wachtende blokkeerde andere soorten — opgelost.
- Ook: `sampler` (drukmeters), `runtimes` (CCD0/CCD1 + pinning), `platform` (Windows Job Objects/affiniteit/prioriteit/idle), `memwatch` (RAM-budget + RSS-bewaking), `hddqueue` (elevator), `resilience` (circuit breaker, bulkhead), `stall` (livelock + retry-amplification), `cache` (single-flight, negative cache).
- Windows-typecheck in de cloud (zig) vond 1 fout (feature Win32_Security) — opgelost. 89 kern-tests groen.
- Open (PC): Windows-affiniteit/prioriteit/Job Objects/idle-tijd echt testen.

## 2026-11-02 (W05 geheugen kern)
- `vortex-memory`: schema 0003, schrijfpipeline (policy, chunken met koppen/code, contextbeschrijving, embeddings, dedup/merge, conflicten, entiteiten/claims/graph, samenvatting, herkomst), hybride ophalen (vector+BM25+graph+temporeel, RRF, rerank, MMR, tegenbewijs, scoreherkomst), vergeten met doorwerking, verouderen, werkruimtes, versies, export/import, backup/restore, decay, sessiesamenvatting, wachtrij als het model weg is.
- Koppeling met modellen in `vortex-context::adapters`; chat haalt herinneringen op (max 1,5 s) en schrijft daarna; GUI: Memory-pagina + live Retrieved memory-paneel.
- W05-gate groen met hash-embedder: recall@3 ≥ 9/10, vergeten overal, backup/restore. Gevonden: policy keek naar het modelantwoord ("remember") → opgelost.
- 1060-config gesplitst in drie servers. Open: LanceDB-vraag + "vergeten" in event log (OPEN-VRAGEN); echte embedder op PC.

## 2026-11-02 (W06 eval + observability)
- `vortex-eval`: sets (7 suites, NL) + retrieval-set, 10 checktypes, runner per versie in DB, vergelijken (ROOD/GROEN), retrieval-eval, lekkagecontrole, perf-harnas. CLI `vortex eval`.
- Server: eval-API (`VortexSubject`), feedback (correctie → gebruikersfeit), trace per run (wachten vs uitvoeren, inference), replay, analytics, context-event (Inspector). GUI: dashboard "Hoe slim is Vortex?", eval-runner, Inspector (context + trace), replay, feedbackknoppen.
- Gevonden: zoeken 17,8 ms/100k → parallel 7 ms; meldingsvloed bij eval → één samenvatting.
- 🔒 `vortex perf` en `vortex soak` klaar, niet volledig gedraaid (regel 7).

## 2026-11-02 (W07 cognitie + context)
- `vortex-cognition`: 8-fasencyclus met events, Global Workspace + doelstapel (dead-task elimination), router (heuristiek + LM-label bij twijfel), metacognitie (snel/diep, effort, budget, selective prediction, switching), plan-and-execute + reflexion + ReAct, salience, leeragenda, user model.
- `vortex-context::assembler`: budget per sectie, compactie bewaart exacte gegevens (gate-test), gists + sessiesamenvatting, geïsoleerde rolcontext, prompt caching (`cache_prompt`).
- Chat = `ChatEngine::run` (cognitive.rs); bijsturen tijdens streamen. GUI: agentpaneel live (fasen/plan/verificatie), Enter = bijsturen tijdens antwoord, leeragenda op dashboard, "Over jou" op Memory-pagina.
- Gevonden: "SQLite" werd code (substring "sql"); vragen-terug bij ≤2 woorden brak korte vragen → nu alleen sturing. 12 server-tests + 13 cognitie-tests groen.

## 2026-11-02 (W08 tools + skills)
- `vortex-tools`: registry + contracten, getypeerde acties, pre/postcondities, idempotentie-ledger (migratie 0006), transacties met diff/preview, effectanalyse (parallel/sequentieel + chaining), sandbox (tijd/CPU/geheugen/uitvoer), 17 ingebouwde tools, MCP-client (stdio + HTTP/SSE), discovery, filters + cache, herstelhints, capability handles.
- Skills: eigen formaat + converters Claude ⇄ Cursor ⇄ AGENTS.md, selectie (trefwoord/embedding/glob), hot-reload, scripts in sandbox, statistiek + uitzetten, voorstellen uit geslaagde runs.
- Chat gebruikt tools (toolrondes via de executor); GUI: live tool-kaarten met diff + Instellingen → Tools & skills. `config/tools.toml`.
- Gevonden: hot-reload miste bestanden (debounce te vroeg) → naloop-debounce; diff-volgorde; `app/**`-glob. 28 tools-tests + 13 server-tests groen; Windows-check groen.

## 2026-11-02 (W09 debatteam)
- `vortex-agents`: rollen in `config/roles/` (10, bewerkbaar), teamsamenstelling door metacognitie, Solvers ∥ met diversiteit, Fact-checker (lokaal + web als online), Critic, Specialist (code/rekenen echt gedraaid), verifier-selectie, Judge-gate met rondes, Synthesizer-instructie, bewijsoverzicht.
- Conflict-team: regels (gebruiker wint, periodes), team (onderzoekers, bronbeoordelaar, Judge), "betwist", rollback (migratie 0007); voorgrond in de chat + achtergrond-worker.
- GUI: agentpaneel met het echte team, Judge in de Verdict-balk, bewijskaart onder antwoorden, conflict-team/terugdraaien in het register. Eval: set `moeilijk` + `--mode quick|deep`.
- Schijf vol in de cloud: `target/` 16 GB → `profile.dev debug = "line-tables-only"` → 2,5 GB. Alle tests groen; Windows-check groen.

## 2026-11-02 (W10 research + ingest)
- Worker: parsers (EPUB/PDF/DOCX/MD/HTML/code) met exacte locators + gRPC Parse/Ingest (JSONL op de koude schijf). Kern: ingest als run → geheugen met locator, incrementeel, hoofdstuk-/boeksamenvattingen; Rust-fallback zonder worker.
- `vortex-crawler`: frontier in SQLite, robots.txt, tempo per domein, Ranker (VOI), Curator (simhash, nieuwheid), cache, PDF → ingest.
- Research-team (`vortex-agents::research`): 11 rollen, 8 stappen, 3 modi, records met schema's → Archivist, triangulatie, rapport met [n] + getallentabel, corpus-synthese. Chat: pad Research. GUI: Research-pagina + Documenten-tab; export CSV/JSONL/Parquet.
- Gevonden: records uit één URL maakten elkaar "verouderd" (geen vaste bron-hash) → opgelost; `uv run` liet een wees-worker achter in tests → venv-binary direct. Gate 1 (EPUB via worker, locator) groen.

## 2026-11-02 (W11 neurale layers)
- `vortex-neural`: eigen ONNX-runtime (MLP-operatoren) + `LayerRegistry` (versies, schaduw, promotie/rollback, hot-swap) achter de `LayerScorer`-haak in vortex-core.
- 7 layers aangesloten (router, importance, usefulness, entity, salience, skill, decay) met vaste featurevectoren; feature logging per beslissing; datasets met decontaminatie; LM-bootstrap-labels.
- Worker: `train_layer.py` (PyTorch of numpy → ONNX) + gRPC Train. Server: trainen → eval → auto-promotie/rollback, idle-training, reranker-dataset + script; dashboard-paneel.
- Torch niet installeerbaar in de cloud (alleen CUDA-wheels) → numpy-trainer als terugval. Gate getest (Rust + end-to-end via worker).
- Gevonden: entity matcher binnen een schrijftransactie + event loggen = deadlock → entiteiten vooraf oplossen én registry logt via eigen schrijfthread.

## 2026-11-02 (W12 geheugen gevorderd)
- Temporele kennisgraaf (geldigheid via herinnering) + regelmotor (transitief/symmetrisch/invers/ketens) + consistentie → conflicten.
- Dromen: samenvoegen, dag/week/maand-samenvattingen, decay, reflectie, redeneren, vertrouwen bijwerken; gate-test (MRR beter). Meta-memory + gaten → leeragenda; procedural memory; few-shot uit 👍.
- Tiering SSD/HDD met zstd-blokken, SSD-cache, prefetch, terughalen; 🔒 `vortex tierbench`. Zelfmodel uit eval → metacognitie. Media-referenties. Experiment Bayes vs kwantum-achtig (kwantum wint bij gekopieerde bronnen, Bayes bij onafhankelijke).
- Gevonden: dedup bij schrijven keek ook naar verouderde herinneringen; feedback heette good/bad (niet up/down) in de W11-datasets → beide opgelost.

## 2026-11-02 (W13 training + vliegwiel)
- Datasets: SFT (team-distillatie, 👍, correcties), DPO (gekozen/verworpen; correctness vs preference), Q&A uit documenten; decontaminatie + versies. Kandidaten van het debat worden nu vastgelegd.
- Worker `train_lora.py` (Unsloth/PEFT + TRL, checkpoints, GGUF-conversie, droogloop) + gRPC `lora:*`; server: adapter-registry, training alleen in idle met pauzeren/hervatten, hot-swap via `/lora-adapters`, schaduwrun, poortwachter, kalibratie, vliegwiel. Layers `verifier` + `prm`. GUI: Training & evals.
- Gate-logica getest met nep-llama-server; echte QLoRA kan alleen op de PC (geen torch in de cloud).

## 2026-11-02 (W14 wereldkennis)
- `vortex-crawler::world`: 12 bronsoorten met voorwaarden + tempo, opslag met dedup, inhaal-modus, event-clustering (gemeld vs bevestigd), bronbias, Wikidata-koppeling, gevolgde sites; crawler: headless browser voor JS-pagina's + layer `crawl_rank`.
- `vortex-agents::world`: KG-integratie, hypotheses, voorspellingen + automatisch toetsen + kalibratie, dag/week/maand-samenvattingen, spelers (alleen publiek). Server: `world_api` + planner (P3-runs); GUI: Research → Wereldmonitor.
- Worker `hf_datasets.py` (schema, mapping + bevestiging, MinHash, decontaminatie, licentie, 5 bestemmingen) + catalogus. Alle tests groen; Windows-check groen.
- Cloud-proxy blokkeert nieuwsbronnen en huggingface.co → W14.01/W14.09 live op de PC verifiëren.

## 2026-11-02 (W15 autonomie)
- `vortex-server::autonomy`: taakqueue (prioriteit, afhankelijkheden, backoff, hervatten na herstart), projecten met notities, triggers (cron met inhalen, interval, event), executor via de Dirigent; leeragenda in idle; monitoring → suggesties → taak.
- Dagelijkse briefing, backups met restore-test + `vortex restore`, voortgangsdashboard (trend → leeragenda), OpenAI-compatibele API (`/v1`), Tailscale-listener, GPU-bewaker voor beheerde llama-servers (getest met testdubbel). GUI: tab Autonomie + meldingen.
- Gebruiker: mobiele weergave hoeft niet → geschrapt. Gevonden: interval-trigger vuurde pas na één interval i.p.v. direct → opgelost.

## 2026-11-02 (W16 multimodaal)
- Worker `media.py`: PDF-lay-out + tabellen + coördinaten, DOCX/XLSX/CSV met structuur, OCR met zekerheid, STT/TTS, beeldgeneratie, video (scènewissels + transcript op tijd), ColPali; `media:*`-jobs. Ingest gebruikt tabellen en spreadsheets.
- Kern `media.rs`: uploads, bijlagen in de chat (visie met ROI, OCR-onzekerheid naar het model, audio, video, documenten + regio's) → geheugen met locators; `/beeld`, voorlezen, paginaweergave. `Message.images` + `--mmproj`.
- GUI: bijlagen uploaden, spraakmodus (opnemen, voorlezen, onderbreken), 🔊 per bericht, Studio-pagina, bronweergave met gemarkeerde coördinaten.
- Test-engines (zelfde contract) maken alles end-to-end testbaar; echte modellen kunnen niet in de cloud (HF geblokkeerd, geen GPU) → W16.01/02/04/05/06 op de PC.

## 2026-11-02 (W17 kwaliteit + performance)
- Record/replay voor tools, semantische cache (strikt, stampede, invalidatie), varianten → schaduw-runs, A/B (gepaarde statistiek), metamorf + counterfactual, positionele tests, compressie-experiment (MPO vs int8, alleen eigen layers).
- Fault-proxy + kleine fault-tests; 🔒 `vortex faults` en 🔒 `vortex ablate` gebouwd, niet gedraaid. Speculative decoding alleen bij gemeten winst (`vortex specbench`), logprob-diagnostiek, CSE/stapcache/VOI/kostengebaseerde planning.
- Programmatic tool calling, git/repo-tools + repo-indexering, browser (Playwright, getest), computer use (PC), cloud-escalatie (standaard uit), visuele UI-controle → 2 echte GUI-bugs gevonden en opgelost.
- Gebruiker: daarna `install.bat` + `start_vortex.bat`.

**Actieve wave:** PC-verificatie W00–W17 · cloud: volgende = W18 (frontier)
