---
type: todo
generated: true
---
# TODO — alle taken van Vortex (gegenereerd)

> **Niet met de hand bewerken.** Bron = `waves/*.md`. Afvinken doe je in de wave-file,
> daarna `python tools/vault/vault.py todo`. Legenda: `[ ]` open · `[~]` bezig · `[x]` af.
> Functionaliteiten per domein (wat Vortex *kan*): [[FUNCTIES]].

**Totaal: 160/235 af.**

| Wave | Status | Af | Open |
| --- | --- | --- | --- |
| [[waves/W00-fundament\|W00 — Fundament]] | bezig | 5 | 15 |
| [[waves/W01-runtime-core\|W01 — Runtime kern]] | bezig | 16 | 2 |
| [[waves/W02-modelbeheer\|W02 — Modelbeheer]] | bezig | 4 | 12 |
| [[waves/W03-gui-chat\|W03 — GUI en chat]] | bezig | 11 | 4 |
| [[waves/W04-dirigent-performance\|W04 — Dirigent en performance]] | bezig | 5 | 2 |
| [[waves/W05-geheugen-kern\|W05 — Geheugen kern]] | bezig | 12 | 2 |
| [[waves/W06-eval-observability\|W06 — Eval en observability]] | bezig | 11 | 2 |
| [[waves/W07-cognitie-context\|W07 — Cognitie en context]] | af | 12 | 0 |
| [[waves/W08-tools-skills\|W08 — Tools en skills]] | af | 15 | 0 |
| [[waves/W09-debatteam\|W09 — Debatteam]] | af | 11 | 0 |
| [[waves/W10-research-ingest\|W10 — Research en ingest]] | af | 14 | 0 |
| [[waves/W11-neurale-layers\|W11 — Neurale layers]] | af | 8 | 1 |
| [[waves/W12-geheugen-gevorderd\|W12 — Geheugen gevorderd]] | af | 9 | 1 |
| [[waves/W13-training-vliegwiel\|W13 — Training en vliegwiel]] | af | 8 | 1 |
| [[waves/W14-wereldkennis\|W14 — Wereldkennis]] | af | 7 | 2 |
| [[waves/W15-autonomie\|W15 — Autonomie]] | af | 9 | 1 |
| [[waves/W16-multimodaal\|W16 — Multimodaal]] | af | 3 | 5 |
| [[waves/W17-kwaliteit-performance\|W17 — Kwaliteit en performance]] | open | 0 | 17 |
| [[waves/W18-frontier\|W18 — Frontier]] | open | 0 | 8 |
| [[waves/W19-ethics-security\|W19 — Ethics & security layer]] | geparkeerd | 0 | 0 |
| [[waves/W20-trading\|W20 — Trading]] | geparkeerd | 0 | 0 |

## W00 — Fundament
Bron: [[waves/W00-fundament]] · status: `bezig`

**Checks**
- [~] `W00.01` SSD: **NVMe 250 GB** (bevestigd 2026-11-02); nog noteren: Windows erop?, vrije ruimte, HDD-groottes + schijfletters → [[04-HARDWARE-EN-OPSLAG]]
- [ ] `W00.02` NVIDIA-driver; `nvidia-smi` toont 5060 Ti + 1060
- [ ] `W00.03` llama.cpp-build die Blackwell én Pascal ondersteunt (anders twee builds)
- [ ] `W00.04` CUDA-toolchain kiezen: CUDA 13 ondersteunt niets onder Turing (1060 valt af); 5060 Ti (sm_120) vraagt CUDA 12.8+. Verwacht: CUDA 12.8/12.9 voor beide (sm_61 + sm_120) — verifiëren met een testkernel op beide kaarten. PyTorch-wheel voor Pascal verifiëren vóór installeren
- [ ] `W00.05` Laadtijd per model gemeten (in [[05-MODELLEN]]); TTFT en t/s gemeten → prestatiedoelen vastzetten in [[modules/orkestrator-performance]]
- [ ] `W00.06` WSL2 installeren en Unsloth/PyTorch-training kort testen: Windows native vs WSL2 (snelheid, VRAM, stabiliteit) → besluit in [[03-BESLUITEN]]

**Installeren**
- [ ] `W00.07` Git, Rust (rustup), Node LTS + pnpm, Python 3.12 + uv, VS Build Tools (C++), WebView2/Tauri-vereisten, protoc, `just`
- [ ] `W00.08` SearXNG (Docker of native) — kan ook in W08

**Repo**
- [ ] `W00.09` Repo `syneyexx/Vortex` geklond op de SSD (`C:\dev\Vortex`, buiten OneDrive); grote data-paden op HDD vastgelegd in `config/` en [[04-HARDWARE-EN-OPSLAG]]
- [x] `W00.10` Rust-workspace met alle crates uit [[20-CODEMAP]] (incl. `vortex-types`, `vortex-crawler`), leeg maar compilerend, afhankelijkheidsrichting afgedwongen — **af 2026-11-02:** 13 crates, `tools/check_deps.py` dwingt richting af
- [x] `W00.11` `proto/` met eerste contracten (kern ⇄ worker, kern ⇄ GUI) volgens [[15-CONTRACTEN]]; codegen voor Rust, Python en TS — **af 2026-11-02:** 5 contracten; Rust (protox, geen protoc), Python (grpcio-tools), TS (buf + protoc-gen-es)
- [~] `W00.12` Python-project `worker/` (uv), Tauri-app `app/` — worker af (gRPC Health/Cancel, 3 tests); app-skelet + iconen af, `pnpm build` groen — **Tauri-build nog verifiëren op de PC**
- [x] `W00.13` `config/` (models.toml, prompts/, roles/, skills/, paths.toml), `evals/`, `design/`; `CARD.md` per crate (template in `VAULT/templates/`)
- [x] `W00.14` `justfile` met vaste commando's: `build`, `test`, `check` (fmt + clippy + ruff + eslint + `vault.py check`), `run`, `eval`, `models-up`, `models-down`
- [~] `W00.15` `vortex` CLI (binary in `vortex-server` of eigen crate) — begin: `vortex status` (GPU's, modellen, health); groeit per wave mee (eval, model laden, runs bekijken/stoppen) — `vortex status/serve/runs` werken; GPU-weergave **verifiëren op de PC** (nvidia-smi)
- [x] `W00.16` Lint/format: clippy, rustfmt, ruff, eslint/prettier

**Modellen**
- [ ] `W00.17` Downloaden naar SSD: gpt-oss-20b (GGUF), Qwen3-Embedding, Qwen3-Reranker-0.6B, Qwen3.5-2B/4B
- [ ] `W00.18` `llama-server` op 5060 Ti (gpt-oss-20b, FlashAttention, KV-quant)
- [ ] `W00.19` `llama-server` op 1060 (embedding + reranker + router)
- [~] `W00.20` Start/stop-scripts (`just models-up/down`); health-endpoint getest — `scripts/models-up.ps1` / `models-down.ps1` + `just models-up/down` geschreven — **testen op de PC**

## W01 — Runtime kern
Bron: [[waves/W01-runtime-core]] · status: `bezig`
- [x] `W01.01` SQLite (WAL) + migratiesysteem met schemaversie
- [x] `W01.02` Event log: append-only tabel, event-typen (bericht, plan, toolcall, retrieval, debat, oordeel, feedback, model/promptversie, lifecycle), outbox + dispatcher, idempotente ontvangers
- [x] `W01.03` Feature-logging-velden voor training (features, keuze, uitkomst) vanaf nu in elk beslis-event
- [x] `W01.04` Run state machine met overgangsvoorwaarden + tests per overgang
- [x] `W01.05` Structured concurrency (scopes, opruimen, losgekoppelde taken met eigenaar)
- [x] `W01.06` Cancellation door hele keten met drie statussen
- [x] `W01.07` Deadline propagation + budgetten (stappen, tokens, tijd)
- [x] `W01.08` Bounded queues + backpressure
- [x] `W01.09` Resource ownership registry (GPU-slot, proces, handle, lock)
- [x] `W01.10` Allocator (mimalloc/jemalloc), begrensde kanalen/caches overal, tokio-console
- [x] `W01.11` Checkpoints + hervatten na crash (crash-test)
- [x] `W01.12` Optimistic concurrency + snapshot-reads
- [x] `W01.13` Config-systeem: versies, hot-reload, run houdt eigen versie
- [~] `W01.14` Secrets broker (OS-keystore), nooit in context/log — broker + redactie af en getest (memory-backend); **OS-keystore (Windows Credential Manager) verifiëren op de PC**
- [~] `W01.15` Subprocess-supervisie (procesgroepen, outputlimieten) — af en getest op Linux (procesgroep, limiet, boom stoppen < 3 s, timeout); **Windows-pad (taskkill /T) verifiëren op de PC**
- [x] `W01.16` Tracing (trace/span-IDs) in alle bovenstaande
- [x] `W01.17` Crash-rapport: wat voltooid / liep / gestopt / extern mogelijk uitgevoerd / hervatbaar / ingrijpen nodig
- [x] `W01.18` Eenvoudige GPU-slot-scheduler: één taak tegelijk op de 5060 Ti, chat voorrang, wachtrij (volledige Dirigent in [[waves/W04-dirigent-performance]])

## W02 — Modelbeheer
Bron: [[waves/W02-modelbeheer]] · status: `bezig`
- [x] `W02.01` Model-registry (models.toml) met alle velden, hot-reload, validatie
- [~] `W02.02` `Backend`-trait + OpenAI-compatibele adapter (chat, stream, embeddings, rerank) — code af, getest tegen nep-server (chat, stream, embeddings, rerank, annuleren) — **verifiëren met echte llama-server op de PC**
- [x] `W02.03` Capability negotiation per backend
- [~] `W02.04` Chat-template- en tokenizer-contract per model + validatie — template-contract via `/props` (geen template = fout; geen tool-rol = tools uit) — **verifiëren per echt model**
- [x] `W02.05` Toolcall-parser per model met versie; ongeldig = fout-event — **af:** `openai-native/v1` + `hermes-xml/v1`, schema-validatie; het `tool.call.invalid`-event wordt gezet door de toollaag (W08)
- [~] `W02.06` Structured output / grammar-constrained generation — json_schema/json_object/GBNF verstuurd + JSON-controle — **verifiëren op echte server**
- [~] `W02.07` Lifecycle manager (7 fasen) + readiness-checks + draining — 7 fasen + readiness (tokenize, eerste generatie, protocol) + draining, getest met test-dubbel — **echte modellen op de PC**
- [~] `W02.08` Wisselen op 5060 Ti: laden/ontladen, RAM-cache, voorspellend laden, bundelen, terug naar thuismodel na idle — wisselen, terug naar thuismodel na idle, bestand voorverwarmen (OS-cache) af; bundelen = model-affiniteit in de Dirigent (W04) — **wisseltijden meten op de PC**
- [~] `W02.09` KV-cache save/restore bij wissel — `/slots` save/restore bij wissel, getest met test-dubbel — **verifiëren met llama-server `--slot-save-path`**
- [ ] `W02.10` MoE-expert-offload-profiel voor Deep-model: benchmark `--n-cpu-moe` vs `-cmoe` + `--moe-cache-mib` (hit-rate in tracing)
- [ ] `W02.11` Quant-shootout per model op eigen eval: Q4_K_M (imatrix) vs NVFP4 (llama.cpp ≥ b8967) vs evt. EXL3; kiezen op kwaliteit × tok/s × VRAM
- [ ] `W02.12` 1060-team als int8-quants (geen FP16 op Pascal)
- [ ] `W02.13` Speculative decoding/MTP proberen waar model het ondersteunt; alleen aan bij gemeten winst
- [x] `W02.14` Incrementele UTF-8 + stopsequenties over fragmentgrenzen + toolargumenten-fasen
- [~] `W02.15` Metingen naar tracing: wachttijd, TTFT, prefill/decode t/s, inter-token, piek-VRAM, laadtijd; NaN/inf-detectie — TTFT, wacht, prefill/decode t/s, inter-token, NaN/inf af; **piek-VRAM (nvidia-smi) nog toevoegen op de PC**
- [ ] `W02.16` Specialisten geregistreerd en getest: Deep, Code, Visie (laden + readiness)

## W03 — GUI en chat
Bron: [[waves/W03-gui-chat]] · status: `bezig`

**Server**
- [x] `W03.01` `vortex-server`: eventstream met volgnummers, snapshot + delta's, resume na reconnect
- [x] `W03.02` Getypeerde frames; verbinding weg ≠ taak gestopt (runpolicy) — frames = getypeerde events uit het log (JSON over WebSocket)
- [~] `W03.03` UI-event coalescing; artefacten als referentie — coalescing af (server 40 ms/512 tekens, client per animatieframe); artefacten als referentie volgen met tools (W08)

**Frontend**
- [x] `W03.04` Design tokens-bestand (kleuren, radii, spacing, typografie, schaduw/glow)
- [x] `W03.05` Topnav + subtabs + routing voor alle pagina's (lege staten waar functie later komt — nooit nep-data)
- [x] `W03.06` Vortex WebGL-shader (geanimeerd, performant, pauzeert als tab inactief)
- [x] `W03.07` Chat: streaming berichten, markdown, codeblokken (highlighting, kopiëren, diff), tabellen, LaTeX, Mermaid
- [~] `W03.08` Invoerbalk: bijlage, Quick/Deep/Research/Adaptive, modelkiezer, strategie, stopknop (echte cancellation) — alles af incl. echte stop; bijlagen = tekstbestanden ≤ 200 kB; modelkiezer stuurt het model mee, **echt wisselen via `ModelServer` op de PC koppelen**
- [x] `W03.09` Tool-kaarten en Agent collaboration-paneel aangesloten op events (vult zich vanaf W08/W09)
- [x] `W03.10` Achtergrondtaak-balk + Lopend werk-pagina (pauzeren/stoppen) — **af:** pauzeren/hervatten via Dirigent (`/api/runs/:id/pause|resume`), stoppen, Dirigent-kaart op Lopend werk
- [~] `W03.11` Statusbalk: gezondheid per subsysteem, beide GPU's live VRAM, actief model, tok/s sparkline — af; GPU-balken lezen nvidia-smi — **verifiëren op de PC**
- [~] `W03.12` Instellingen: system prompt + rol-prompts, live vanaf volgend bericht, "nu toepassen", versiegeschiedenis + terugzetten; paden; modellen; gedrag — system prompt + rol-prompts live, versies, terugzetten af; paden/modellen/gedrag nog via `config/*.toml` (formulieren later)
- [x] `W03.13` Sessies: opslaan/herstellen, branching/forking, nieuwe chat, geschiedenis
- [x] `W03.14` Notificaties voor achtergrondtaken
- [x] `W03.15` Visuele vergelijking met referentie (screenshot naast elkaar) — Playwright-screenshot naast referentie; gevonden en opgelost: herladen tijdens streamen verloor tekst, kolom-overflow, shader-naad. **Herhalen met echte GPU-data op de PC**

## W04 — Dirigent en performance
Bron: [[waves/W04-dirigent-performance]] · status: `bezig`
- [x] `W04.01` Dirigent volgens [[modules/orkestrator-performance]]: taakaanmelding met middelenvraag, P0–P3, admission control per middel, preemption op veilige punten, priority inheritance, model-affiniteit, pipelining, bulkheads, backpressure, drukmeters, idle-detectie — **af:** `vortex-core::dirigent` (12 tests) + `sampler` (drukmeters, idle, externe GPU-last) + pauzeren/hervatten (`hold`); RunContext::gpu loopt via de Dirigent. Idle-tijd op Windows (GetLastInputInfo) typecheckt — gedrag **verifiëren op de PC**
- [~] `W04.02` Twee tokio-runtimes (CCD0 interactief / CCD1 achtergrond) + CPU-affiniteit en Windows-prioriteit per proces — twee runtimes met core-pinning + CCD-indeling (Windows- en Linux-nummering) + `place_process` af en getest op Linux; Windows-affiniteit/prioriteit typecheckt — **verifiëren op de PC**
- [~] `W04.03` RAM-budget per modus + harde caps (Job Objects) + nette herstart na checkpoint — RAM-budget per modus + RSS-bewaking met event/callback af en getest; harde cap via Job Object (Windows) typecheckt — **verifiëren op de PC**
- [x] `W04.04` HDD: één I/O-wachtrij, gesorteerd, sequentieel
- [x] `W04.05` Circuit breakers + bulkheads
- [x] `W04.06` Stall/livelock- en retry-amplification-detectie (signalen + events)
- [x] `W04.07` Single-flight + negative caching utilities

## W05 — Geheugen kern
Bron: [[waves/W05-geheugen-kern]] · status: `bezig`
- [x] `W05.01` `MemoryStore`-trait; SQLite-schema (herinneringen, typen, bron, tijd, validity, confidence, salience, versie, werkruimte, edges, FTS5)
- [~] `W05.02` LanceDB-index (int8, dims configureerbaar) + embedding-metadata per vector + index-generaties — **afwijking:** eigen int8-index (`FlatI8Index`, in SQLite, zelfde transactie) i.p.v. LanceDB achter `VectorIndex`-trait; generaties + metadata af — zie [[OPEN-VRAGEN]]
- [~] `W05.03` Embedding-service (1060) met single-flight en batching — batching + single-flight + MRL-inkorting af en getest; **echte Qwen3-Embedding op de 1060 verifiëren op de PC**
- [x] `W05.04` Schrijfpipeline: policy (heuristisch), chunking, contextbeschrijving, entiteiten/claims (router-model), graph-koppeling, salience, bron, samenvatting, herkomstafhankelijkheden
- [x] `W05.05` Dedup/merge (similarity-drempel) + conflictdetectie → conflictregister (team volgt in W09)
- [x] `W05.06` Short-term, long-term, episodic, semantic, user/project-geheugen
- [x] `W05.07` Ophalen: query rewriting, vector + FTS5 + graph (multi-hop) + temporeel, samenvoegen, rerank (Qwen3-Reranker), MMR, tegenbewijs-slot, ranking relevantie+recency+importance, filters, scoreherkomst — rerank via Qwen3-Reranker zodra de 1060-server draait (adapter af)
- [x] `W05.08` Tool results → geheugen via policy
- [x] `W05.09` Deletion propagation (alle indexen, samenvattingen, caches) + "vergeet dit"
- [x] `W05.10` Bron gewijzigd → afgeleiden verouderd
- [x] `W05.11` Project-werkruimtes (isolatie + gedeelde algemene kennis), cache-isolatie
- [x] `W05.12` Export/import, backup → HDD, restore getest — restore getest; backup standaard naar `paths.cold_dir`
- [x] `W05.13` Memory-pagina in GUI: bekijken, bewerken, vergeten, filters, conflictregister, Retrieved memory-paneel live
- [x] `W05.14` Basis decay + samenvattingen

## W06 — Eval en observability
Bron: [[waves/W06-eval-observability]] · status: `bezig`
- [x] `W06.01` Eval-runner (CLI + GUI-knop), resultaten per versie in DB
- [x] `W06.02` Eval-sets: redeneren, code (met tests), geheugen-recall, research/bronnen, toolgebruik, conflicten — start klein, groeit per wave — 7 suites, 34 taken (NL) + retrieval-set; tools/research `requires` W08/W10 → skip tot die er zijn; groeit per wave
- [x] `W06.03` Retrieval-eval: recall@k, rang, irrelevant, geen-antwoord-vragen
- [x] `W06.04` Lekkagecontrole (inhoud + herkomst, near-duplicates)
- [x] `W06.05` Trace-viewer (spans per run), wachttijd vs uitvoering, inference-metrics
- [x] `W06.06` Context Assembly Inspector + "waarom opgehaald"
- [x] `W06.07` Feedback-knoppen in chat → events
- [x] `W06.08` Session replay uit event log
- [x] `W06.09` Error analysis-overzicht, tool/memory analytics
- [x] `W06.10` Dashboard-begin: scores over tijd
- [~] `W06.11` Performance-tests voor alle prestatiedoelen (TTFT, retrieval p95, stop-latency, 60 fps, idle-CPU) — achteruitgang = rood — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag; suite draaien op commando) — harnas `vortex perf` af (korte check OK: retrieval p95 7 ms, stop 13 ms, idle-CPU 0 %) — volledige suite 🔒 op commando
- [~] `W06.12` 24-uurs soak-test met geheugen-telemetrie per proces; vlakke RSS-trend vereist — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (harnas bouwen mag) — harnas `vortex soak --minutes 1440` af (RSS-trend + CSV) — 🔒 24 uur alleen op commando
- [x] `W06.13` Profiling-setup: flamegraphs (Tracy/Superluminal), criterion-benchmarks hete paden — criterion-benches (vectorzoeken, events) + Tracy-feature; bench vond 17,8 ms/100k → geparallelliseerd naar 7 ms

## W07 — Cognitie en context
Bron: [[waves/W07-cognitie-context]] · status: `af`
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

## W08 — Tools en skills
Bron: [[waves/W08-tools-skills]] · status: `af`
- [x] `W08.01` Toolregistry + contracten (schema, bijwerkingen, timeout, annuleerbaar, herhaalbaar, resources) — `ToolRegistry` + `ToolContract` (schema in/uit, effect lezen/lokaal/extern, timeout, annuleerbaar, idempotent, resources, netwerkbestemmingen, herkomst, versie)
- [x] `W08.02` Getypeerde actie-representatie + validatie — `Action` (doel, invoer, verwacht, afhankelijkheden, pre/post) + JSON-schema-validatie met milde coercie
- [x] `W08.03` Pre/postconditions-framework — `Condition` (pad bestaat/afwezig, bestand bevat, uitvoer bevat, veld); postcondities zien de klaargezette transactie
- [x] `W08.04` Idempotentie (taak-ID), transactionele uitvoering, diff/preview, checkpoints — `tool_ledger` (migratie 0006): retry = replay, zelfde ID + andere invoer = geweigerd, onderbroken externe actie nooit blind herhaald; `FileTxn` (staging → diff → atomair toepassen, rollback); dry-run/preview; checkpoint per toolronde
- [x] `W08.05` Effectanalyse → parallel/sequentieel, chaining — `plan::stages` (lezers parallel, schrijvers op zelfde doel/resource na elkaar, barrières) + chaining `$ref`/`${a.text}`
- [x] `W08.06` Sandbox-procesgroep (Python, shell) met resource-limieten — `Sandbox` via de Supervisor: eigen procesgroep, tijd/CPU/geheugen (Unix ulimit, Windows Job Object)/uitvoer-limieten, schone omgeving, annuleren stopt de boom
- [x] `W08.07` Tools: bestanden (lezen/schrijven/bewerken/zoeken), lokale mappen doorzoeken, web search (SearXNG), pagina lezen, code interpreter, SQLite-tool, generiek API-calling framework, geheugen-tools — read/write/edit/delete_file, list_dir, search_files, find_files, web_search (SearXNG), web_fetch (HTML→tekst), python, shell, sqlite_query/exec, http_get/http_request + API's uit `config/tools.toml`, memory_search/write/forget (server)
- [x] `W08.08` MCP-client (stdio + HTTP), MCP-tools in registry — `McpClient` stdio + streamable HTTP (JSON/SSE, sessie-ID), paginering, effect uit annotaties, herverbinden; tools als `mcp__server__tool`
- [x] `W08.09` Tool discovery (naam+beschrijving, schema bij gebruik) — kernset met schema, rest naam+beschrijving in de prompt; `describe_tool` + `call_tool`
- [x] `W08.10` Resultaatfiltering met code, tool result caching — `ResultFilter` (pointer/grep/head/tail/max), automatisch inkorten (begin+eind), `ToolCache` (TTL) voor leestools
- [x] `W08.11` Herstel bij toolfouten — `ToolError` met soort + hint voor het model (schema, bijna-match bij edit, dichtstbijzijnde toolnaam); retry met backoff alleen bij idempotent
- [x] `W08.12` Capability handles per run (technisch) — `CapabilityHandle` (mappen, netwerk, schrijven, processen, toolglobs); standaard alles open (geen remmen tot W19)
- [x] `W08.13` Skills: eigen formaat, converters Claude Skills ⇄ Cursor rules ⇄ AGENTS.md, import/export, progressive disclosure, selectie (embeddings + globs), scripts in sandbox, hot-reload — eigen formaat (superset Claude), converters Claude ⇄ Cursor ⇄ AGENTS.md (heen en terug getest), progressive disclosure (`load_skill`), selectie (trefwoorden + embeddings + globs + alwaysApply), scripts in de sandbox (`run_skill_script`), hot-reload, statistiek + automatisch uitzetten
- [x] `W08.14` Skill-voorstellen uit geslaagde traces (basis) — `propose::observe` na geslaagde runs met ≥ 2 toolstappen; zelfde patroon telt op; accepteren → skill (GUI Instellingen)
- [x] `W08.15` Tool-kaarten in GUI live — tool-kaarten live per run (status, duur, doel; klik = invoer, diff, uitvoer/fout) + beheer in Instellingen → Tools & skills

## W09 — Debatteam
Bron: [[waves/W09-debatteam]] · status: `af`
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

## W10 — Research en ingest
Bron: [[waves/W10-research-ingest]] · status: `af`
- [x] `W10.01` Python-worker ingest-service (gRPC), voortgang-events — `worker`: gRPC `Parse` + `Ingest` (stream: progress, chunk_batch, error, done; annuleren per trace); kern: `ChatEngine::ingest` als run (P2/P3) met events `ingest.job.*`; zonder worker Rust-fallback (Markdown/tekst/HTML/code)
- [x] `W10.02` Parsers: EPUB, PDF (tekst), DOCX, Markdown, HTML, code — `vortex_worker.parsers`: EPUB (zip+OPF, locator "hoofdstuk N > kop"), PDF (PyMuPDF, koppen op lettergrootte, "p. N"), DOCX (zelf, Heading/Kop-stijlen), Markdown ("regel N > kop"), HTML, code (Python-ast / patronen, "regels a-b"), tekst
- [x] `W10.03` Structuur-bewust chunken, contextual chunk descriptions, content-hashes, incrementeel — secties → geheugen-schrijfpijplijn (structuur-chunken, contextbeschrijving, embeddings); content-hash per document → ongewijzigd = overslaan, gewijzigd = oude versie verouderd
- [x] `W10.04` Entiteit/claim-extractie op 1060, graph-koppeling, dedup, conflictcheck — via de W05-pijplijn op het extract-model (1060): entiteiten/claims → graph, dedup, conflictcheck
- [x] `W10.05` Hiërarchische samenvattingen (sectie → hoofdstuk → boek) — hoofdstuk- en boeksamenvattingen (Summary, derived_from, locator)
- [x] `W10.06` Onderwerp-research: plan, deelvragen, zoeken, lezen, iteratieve retrieval, adaptief bijsturen, ontbrekend bewijs zoeken, kruiscontrole — `research::run`: plan → deelvragen → zoeken (geheugen + web) → lezen → records → analyse → trianguleren → Judge/Skepticus (volgende gerichte zoekactie) → rondes
- [x] `W10.07` Webcrawler-team: Scout, Crawler (HTTP), Reader/Extractor, Ranker, Curator; focused crawling met budgetten; frontier in SQLite; ruwe HTML op HDD; per-domein tempo + robots.txt; cache — `vortex-crawler`: Crawler (HTTP, links), Reader/Extractor, Ranker (relevantie + VOI), Curator (exact + simhash, kwaliteit, nieuwheid → stoppen), frontier (`crawl_frontier`), ruwe HTML/tekst/PDF op de koude schijf, robots.txt + crawl-delay + tempo per domein, cache + negative cache; Scout in het research-team
- [x] `W10.08` Research-team volgens [[modules/research-team]]: alle 11 rollen, loop (8 stappen), drie modi, Judge-gate per deelvraag — Lead, Scouts+crawler, Readers, Extractor, Bronbeoordelaar (domeinbetrouwbaarheid + onafhankelijke groepen), Analysts, Fact-checker (trianguleren), Skepticus, Judge per deelvraag, Writer, Archivist; leren via `domain_stats`; modi quick/standaard/deep (`choose_mode`; Research-knop = deep)
- [x] `W10.09` Extractor-schema's (claim, getal, entiteit, relatie, event, tabel, citaat, bron) + validatie + direct naar geheugen via Archivist — `records`: claim, number, entity, relation, event, table, quote, source met JSON-schema + coercie; ongeldige verworpen; Archivist → `research_records` + geheugen (herkomst + locator)
- [x] `W10.10` Parallel: crawlers (CPU) + readers/extractor (1060) terwijl hoofdmodel analyseert — per ronde alle deelvragen tegelijk zoeken (CPU/netwerk) en alle bronnen tegelijk lezen/extraheren (1060); hoofdmodel analyseert daarna
- [x] `W10.11` Research-pagina: live records, deelvragen, bronnen, bewijsoverzicht, export CSV/Parquet — Research-pagina: starten, deelvragen, bronnen + betrouwbaarheid, records live, rapport, export CSV/JSONL/Parquet (pyarrow)
- [x] `W10.12` Brongebonden antwoordopbouw (passage ↔ claim), rapporten/tabellen als artefact — Analyst/Writer met [n]-verwijzingen naar genummerde bronnen (URL of document + locator), getallentabel, bronnenlijst; rapport als `.md`-artefact in data/reports
- [x] `W10.13` Global corpus synthesis — `corpus_synthesis`: map over documentsamenvattingen (1060) → reduce (hoofdmodel) met [Dn]; knop "Hele verzameling"
- [x] `W10.14` Ruwe data op HDD, kennis op SSD — `paths.cold_dir`: crawl (raw/text/docs) en ingest-JSONL; kennis (SQLite/index/rapporten) in data_dir

## W11 — Neurale layers
Bron: [[waves/W11-neurale-layers]] · status: `af`
- [x] `W11.01` Feature-extractie uit event log → datasets per layer (versie, decontaminatie) — `LayerScorer::decide` logt elk voorbeeld (`neural.example.logged`: x, heuristiek, referentie); `build_examples` koppelt uitkomsten (gebruik, feedback, labels); `dataset::write`: versie = inhoudshash, decontaminatie (8-grammen eval-set), vaste train/test-split
- [x] `W11.02` Bootstrap-labeling met LM — `bootstrap` (router, salience, entity, skill, usefulness): LM labelt voorbeelden zonder uitkomst → `neural_labels` (bron lm/human/outcome); knop "LM-labels"
- [x] `W11.03` Trainingsscripts (PyTorch) per layer + ONNX-export — `worker/train_layer.py`: PyTorch (GPU als die er is) of numpy-trainer, gewogen verlies, early stopping, ONNX-export (Sub/Div/Gemm/Relu/Sigmoid|Softmax + metadata); gRPC `Train` (`layer:<naam>`)
- [x] `W11.04` `vortex-neural`: ONNX laden via `ort` (1060/CPU), hot-swap van versies — eigen pure-Rust ONNX-runtime (MLP-operatoren, µs; i.p.v. `ort`: geen native DLL nodig) + `LayerRegistry` met hot-swap (manifest per layer, versies)
- [x] `W11.05` Layers: router-net, importance scorer, entity matcher, salience-net, usefulness-predictor, skill selector, decay-predictor — 7 layers met vaste featurevectoren en haken: router (`route_with`), importance (geheugen schrijven), entity matcher (entiteiten samenvoegen), salience (`tag_with`), usefulness (eindscore retrieval), skill selector, decay-predictor
- [~] `W11.06` Reranker fine-tune op eigen feedback — `POST /api/neural/reranker/dataset` (vraag, passage, label uit retrieval + feedback) + `worker/scripts/train_reranker.py` (LoRA op Qwen3-Reranker, P("yes")) → **trainen zelf op de PC** (PyTorch)
- [x] `W11.07` Schaduwmodus + automatische promotie/rollback via eval — schaduwmodus (net draait mee, gelogd, overeenstemming geteld); `auto_decide`: promoveren alleen als het net ≥ 2 pt wint van heuristiek én actieve versie; actieve versie die verliest → automatisch terug
- [x] `W11.08` Idle-training (koppeling scheduler) — `spawn_idle_training`: elke 10 min bij idle (Dirigent) en ≥ 50 nieuwe voorbeelden → training als P3-run
- [x] `W11.09` Dashboard: versies en scores per layer — Dashboard → "Neurale layers": modus, versies, eval net vs heuristiek, schaduw-overeenstemming, voorbeelden, acties

## W12 — Geheugen gevorderd
Bron: [[waves/W12-geheugen-gevorderd]] · status: `af`
- [x] `W12.01` Procedural memory (skills + werkwijzen, succesrate, versies) — `sync_procedures`: skills + geleerde werkwijzen als Procedural-herinneringen met succesrate, gebruik en versie (nieuwe versie = oude verouderd); Code/Planning-vragen geven werkwijzen voorrang
- [x] `W12.02` Meta-memory (wat bekend, hoe zeker, gaten) → leeragenda — `meta_report`: per soort aantal + zekerheid, verouderd/koud/conflicten/afgeleid, sterkste onderwerpen, kennisgaten (vaak genoemd, weinig feiten) → leeragenda; tab "Wat weet Vortex"
- [x] `W12.03` Dreaming in idle: consolidatie, clustering/merging, decay/pruning, hiërarchische tijd-samenvattingen, reflectie, contradiction resolution, confidence-updates — `dream`: bijna-duplicaten samenvoegen, dag→week→maand-samenvattingen, decay, reflectie, redeneren + consistentie, vertrouwen ↑ bij onafhankelijke steun / ↓ bij tegenspraak; idle elke ~6 u (P3) + knop; gate-test: retrieval (MRR) beter na dromen
- [x] `W12.04` Temporal knowledge graph: validity from/to, historie — feiten met effectieve geldigheid (edge of herinnering → conflictbeslissingen/rollback werken door); `facts_about(entity, at)`, `fact_history`
- [x] `W12.05` Graph-redeneren (afleidingen, consistentie, regels) — regelmotor zonder LM (transitief, symmetrisch, invers, ketens; `config/rules.toml`), afgeleide feiten met regel + steun, opnieuw berekend; functionele relaties met overlappende waarden → conflict
- [x] `W12.06` Zelfmodel uit eval-resultaten → router/metacognitie — `SelfModel` uit de laatste eval-run per suite (taaktype via router) → metacognitie (zwak = diep pad + extra zorg) + leeragenda `weak_task`; ververst na elke eval
- [~] `W12.07` Tiering SSD/HDD: zstd-blokken, ophalen + SSD-cache, prefetch, decay-gestuurd; test met synthetische TB-schaal — `tiering`: decay-gestuurd (W11-layer of heuristiek) naar zstd-blokken op de koude schijf, stub op SSD, centroid-zoeken, SSD-cache (LRU) + prefetch, treffers terughalen; 🔒 `vortex tierbench --memories N` voor de TB-simulatie (op commando)
- [x] `W12.08` Multi-modaal geheugen (afbeeldingen, bestanden als referentie met embeddings) — `POST /api/memory/media`: afbeelding/audio/video/bestand als referentie met beschreven inhoud (geëmbed), bestand ophalen via `/api/memory/:id/media`; beeld-embeddings (CLIP) volgen in W16
- [x] `W12.09` **Experiment (gebruiker wil testen):** quantum-achtige kansmodellen (niet-klassieke kansregels, volgorde-effecten, interferentie tussen hypotheses) voor onzekerheid, tegenstrijdige bronnen en contradiction resolution; A/B tegen gewone Bayesiaanse aanpak op conflict-eval — `belief`: Bayes vs kwantum-achtig (gedeeltelijke projecties met contexthoeken: volgorde-effecten, interferentie, verzadiging bij kopieën); A/B op synthetische conflict-scenario's (onafhankelijk/kopieën/verouderd), θ/λ op trainsplit; uitslag: kwantum beter bij gekopieerde bronnen, Bayes bij onafhankelijke; dashboard-knop
- [x] `W12.10` Memory-based few-shot (voorbeelden uit geheugen in context) — 👍 op een antwoord → voorbeeld in procedural memory; vergelijkbare vragen krijgen 1–2 voorbeelden in de context

## W13 — Training en vliegwiel
Bron: [[waves/W13-training-vliegwiel]] · status: `af`
- [x] `W13.01` Dataset-builders: SFT (distillatie team-antwoorden), DPO (gekozen/verworpen, voorkeur vs correct), Q&A/redeneervoorbeelden uit ebooks/datasets — `build_sft` (Judge-goedgekeurde team-antwoorden zonder negatieve feedback, 👍-antwoorden, correcties), `build_dpo` (gekozen vs laagst gescoorde kandidaat; label correctness/judge vs preference), `build_qa` (vraag-antwoord + redenering uit ingelezen secties, met locator); dedup + decontaminatie (8-grammen eval) + versie
- [~] `W13.02` QLoRA-pipeline (Unsloth/PEFT) op Qwen3.5-9B safetensors; mergen/converteren → GGUF — `worker/train_lora.py`: Unsloth of PEFT (+bitsandbytes 4-bit) + TRL SFT/DPO, checkpoints, adapter → GGUF (`convert_lora_to_gguf.py`), optioneel mergen → GGUF; `validate()` droogloop; gRPC `lora:sft|dpo` → **echte training op de PC** (PyTorch)
- [x] `W13.03` Adapter-registry: basismodel, datasetversie, instellingen, scores, regressies — tabel `adapters` (basismodel, dataset, instellingen, scores per suite, regressies, schaduw, checkpoint, metrics, status) + `training_datasets` (migratie 0011)
- [x] `W13.04` Adapters los laden/hot-swappen waar backend het kan — `--lora … --lora-init-without-apply` bij start; `OpenAiBackend::{lora_adapters, set_lora}` (llama-server `/lora-adapters`) + LoRA per verzoek; knop aan/uit
- [x] `W13.05` Verifier/reward model + process reward model (tussenstappen) — layers `verifier` (antwoordniveau: citaties, onzekerheid, Judge, critic, claims, objectieve check) in het debat en `prm` (stapniveau) op planstappen; kandidaten vastgelegd (`agent.candidates.recorded`); LM-rewardmodel = DPO-adapter (W13.02)
- [x] `W13.06` Confidence calibrator + kalibratiemeting (gescheiden dev/test) — `Calibrator` (Platt-schaling, ECE/Brier/nauwkeurigheid; dev/test gescheiden) op Judge-zekerheid vs feedback; toegepast in de Judge-gate
- [x] `W13.07` Poortwachter: eval + schaduwrun → promoveren of terug — `gate`: adapter aan → alle suites met basislijn → geen regressie > 2 pt én ergens ≥ 2 pt winst én schaduwrun (blind pairwise Judge op recente vragen) ≥ 50 % → actief, anders afgewezen + adapter uit
- [x] `W13.08` Training alleen in idle, GPU vrijgeven bij gebruik — training als P3-run met GPU-lease; gebruiker actief of preemptie → worker annuleren, checkpoint, status gepauzeerd; vliegwiel hervat/start in idle (≥ min_new_examples)
- [x] `W13.09` Training/evals-pagina in GUI — Modellen → "Training & evals": datasets bouwen, trainen, adapters met scores/regressies/schaduw, poortwachter, aan/uit, kalibratie

## W14 — Wereldkennis
Bron: [[waves/W14-wereldkennis]] · status: `af`
- [~] `W14.01` Connectors: GDELT, RSS (bronnenlijst meerdere landen/richtingen), Wikipedia Current Events, Wikidata, ReliefWeb, USGS, NASA EONET, World Bank, Eurostat, CBS, arXiv, Hacker News — rate limits + voorwaarden per bron — `vortex-crawler::world::sources` (12 soorten, RSS/RDF/Atom via quick-xml, URL's instelbaar), `terms()` = voorwaarden + minimale tijd per soort (tempo per soort), `config/world.toml` (22 bronnen: NL/UK/US/DE/FR/QA/IN/AU/JP, publiek/links/rechts/staats/commercieel), Wikidata-koppeling (Q-ID per event); getest tegen nepservers → **live-check op de PC** (cloud-proxy blokkeert de hosts)
- [x] `W14.02` Gepland ophalen + inhaal-modus — planner (`world_api::spawn_world`, elke minuut): bron aan de beurt per interval; venster = sinds laatste geslaagde keer (max 7 dagen); periodieke stappen (`__resolve`, `__hypothesize`, `__predict`, `__summary_*`) ook persistent → inhalen na herstart; elke cyclus = P3-run via de Dirigent; LM-stappen alleen als de gebruiker niet actief is
- [x] `W14.03` Crawler: headless browser voor JS-pagina's, recrawl-schema voor gevolgde sites, Ranker als neurale layer (data uit W10) — `--headless --dump-dom` (Chrome/Edge/Chromium, `paths.browser` of automatisch) als de HTML vrijwel geen tekst maar wel scripts heeft (getest met echte Chromium); `followed_sites` + `recrawl_due` (wijziging → item); layer `crawl_rank` (9 kenmerken, label = pagina door de Curator bewaard, uit frontier + `crawl.page.fetched`)
- [x] `W14.04` Event-clustering (artikelen → één event), gemeld vs bevestigd — tokens + Dice-gelijkenis in een tijdvenster, toewijzen aan bestaand event of nieuw; **bevestigd** bij ≥ 2 onafhankelijke brongroepen (domein), anders gemeld; thema + regio; idempotent
- [x] `W14.05` Temporal KG-integratie, bronbias — bevestigde events → herinnering (kind World, `valid_from` = eerste melding, bron + Wikidata-entiteiten) → entiteiten/claims in de graaf; bronbias per bron (aandeel in bevestigde events, landen, richting)
- [x] `W14.06` Causale hypotheses + voorspellingen + later toetsen (kalibratie wereldmodel) — hypotheses (steun/contra, vertrouwen), voorspellingen met kans + toetscriteria + datum, automatisch toetsen tegen binnengekomen items (waar/onwaar/niet te beslissen + bewijs), kalibratie (Brier, ECE, per herkomst/speler, Platt bij ≥ 20)
- [x] `W14.07` Dag/week/maand-samenvattingen per thema/regio — per groep, bevestigd vs gemeld gescheiden, extractieve terugval zonder model; ook als herinnering (Summary)
- [x] `W14.08` Spelers per sector: bronnen verzamelen, beslissingen/principes/uitkomsten extraheren, mislukkingen, voorspellingen toetsen — alleen organisaties en publieke figuren met publieke rol; bronnen = wereldmonitor + ingelezen documenten; records (decision/principle/failure/outcome) met bron + locator in het geheugen; "wat zou X doen?" = voorspelling met `actor`, zelfde toetsing + kalibratie per speler
- [~] `W14.09` HF-datasets: streamen, schema, mapping-voorstel + bevestiging, filter, dedup (MinHash), decontaminatie, licentie, bestemming kiezen, catalogus — `worker/hf_datasets.py` (streaming, schema chat/qa/preference/code/text, mapping-voorstel + LM-verfijning, kwaliteitsfilter, exact + MinHash-LSH ook tegen eerdere imports, 8-gram-decontaminatie, licentie uit de card); bestemmingen kennis / SFT / DPO / layer (router) / eval; `hf_catalog`; GUI: bevestigen vóór import; end-to-end getest met lokale bestanden → **HF Hub streamen op de PC** (cloud blokkeert huggingface.co)

## W15 — Autonomie
Bron: [[waves/W15-autonomie]] · status: `af`
- [x] `W15.01` Idle-detectie + GPU direct vrijgeven bij gebruik — idle uit de sampler (OS-invoer + nvidia-smi → Dirigent, W04); P2/P3 wijken al; nieuw: GPU-bewaker voor beheerde llama-servers (`managed = true`): ontladen zodra een ander programma de GPU gebruikt en Vortex niets doet, terugladen na `restore_after_secs`, een chatverzoek laadt meteen (getest met de llama-server-testdubbel)
- [x] `W15.02` Leeragenda zelf uitvoeren in idle — taak `agenda` (trigger elke 4 u, alleen idle): gaten/betwiste feiten → research (quick), zwakke taaktypen → eval-taak; status open → researching → done
- [x] `W15.03` Taakqueue: prioriteit, afhankelijkheden, long-running projecten, multi-session continuity — `tasks` (P1–P3, afhankelijkheden, niet-eerder-dan, herhaalpogingen met backoff, pauzeren/hervatten/annuleren, herstel na herstart), executor (max 2 tegelijk, elke taak een run via de Dirigent), projecten met voortgang en doorlopende notities
- [x] `W15.04` Planning en triggers (tijd, gebeurtenis) — cron (lokale tijd, gemist moment één keer inhalen), interval, event (soort-patroon + `veld=waarde`-filter, event in de taakparams); standaardtriggers aan/uit in de GUI
- [x] `W15.05` Proactieve monitoring en suggesties — elke 30 min: open conflicten, mislukte taken, backup oud/mislukt, volle leeragenda, nieuws dat raakt aan gebruikers-/projectentiteiten, verlopen voorspellingen, schijfruimte → suggestie (één per dag), accepteren = taak
- [x] `W15.06` Dagelijkse briefing — 07:30 (inhalen als de PC uit stond): wereld, wat Vortex leerde, conflicten, taken/projecten, voorspellingen, suggesties, backup; model schrijft, vaste opmaak als terugval; ook als herinnering + melding
- [x] `W15.07` Lokale OpenAI-compatibele API (Cursor/eigen projecten) — `/v1/models`, `/v1/chat/completions` (stream + niet-stream): `vortex[-quick|-deep|-research]` = volledige pijplijn in een eigen sessie per gesprek; andere modellen = doorgeven aan de llama-server
- [~] `W15.08` Telefoontoegang via Tailscale + mobiele weergave — extra listener op het tailnet-adres (`[remote] tailscale = true`, `tailscale ip -4`), geen open poorten; **mobiele weergave vervalt (besluit gebruiker 2026-11-02)**; Tailscale zelf testen op de PC
- [x] `W15.09` Voortgangsdashboard "hoe slim is Vortex" — eval per maand per suite (+ trend), geheugengroei, layers, adapters, leeragenda, taken, kalibratie wereldmodel; achteruitgang > 5 pt → leeragenda (`weak_task`) → vliegwiel; maandelijkse eval-trigger
- [x] `W15.10` Automatische backups + restore-test — 03:15: online SQLite-backup + config naar de koude schijf, bewaren 7/4/12, restore-test (integrity_check, schemaversie, tellingen, proef-terugzetten); `vortex restore <bestand> [--with-config]`

## W16 — Multimodaal
Bron: [[waves/W16-multimodaal]] · status: `af`
- [~] `W16.01` Visie (Qwen3.5-9B): afbeeldingen, screenshots, grafieken; ROI-verwerking (overzicht → uitsnede) — model met rol `vision` + `mmproj` (llama-server `--mmproj`), `Message.images` → OpenAI-contentdelen; `look`: overzicht (1024 px) → model kiest ≤ 3 gebieden → uitsneden op volle resolutie → nauwkeurige beschrijving (tekst letterlijk, assen/legenda's); beelden ook naar het eindmodel als dat visie kan; getest met nep-model → **echt model op de PC**
- [~] `W16.02` OCR (GLM-OCR / PaddleOCR-VL) met onzekerheid doorgeven — worker `ocr` (PaddleOCR → Tesseract; regels met bbox + zekerheid); gemiddelde zekerheid → `confidence` van de herinnering, regels < `ocr_low` als "ONZEKER gelezen" in de prompt (getest: komt bij het model aan); gescande PDF-pagina's → renderen → OCR, anders visie → **engines op de PC**
- [x] `W16.03` Documentverwerking: PDF-lay-out, DOCX, spreadsheets; tabellen met structuurbehoud — PyMuPDF-blokken met coördinaten, leesvolgorde (kolommen), tabellen (`find_tables` → markdown, tabeltekst niet dubbel), scanpagina-detectie; DOCX in documentvolgorde met tabellen; XLSX/CSV als tabellen met herhaalde kop per 50 rijen en cel-locator (`Blad!A1:B51`); ingest gebruikt dit
- [~] `W16.04` Visuele documentretrieval (ColPali-achtig) + coördinaat-bronverwijzingen — regio's (blokken, tabellen) als herinneringen met locator `p. N [x0, y0, x1, y1]`; GUI rendert de pagina met het gebied gemarkeerd; ColQwen2-pagina-embeddings (`colpali_index`/`colpali_query`) in de worker → **GPU op de PC**
- [~] `W16.05` Spraak in (Whisper-klasse, CPU) + uit (TTS), onderbreekbaar, voice-modus in GUI — faster-whisper (CPU int8, segmenten met tijden + zekerheid) en Piper/Kokoro per zin; GUI: 🎙 opnemen → transcriberen → versturen, antwoorden voorlezen, onderbreken (stil/uit, nieuwe opname stopt het afspelen), 🔊 per bericht; audio als bijlage → transcript met tijdstempel-locators in het geheugen → **engines op de PC**
- [~] `W16.06` Beeldgeneratie/-bewerking (na ontladen LLM) — diffusers (tekst → beeld en beeld → beeld); eerst GPU-lease P1 + beheerde llama-servers ontladen, daarna terugladen; `/beeld <prompt>` in de chat (met beeldbijlage = bewerken), Studio-pagina → **diffusion op de PC**
- [x] `W16.07` Video: adaptieve frameselectie + tijdsuitlijning met audio/transcript — ffmpeg-scènewissels + min/max-interval + uitdunnen; audiospoor → transcript; elk frame krijgt de spraak tot het volgende frame; frames → visie; tijdlijn met locators `mm:ss–mm:ss` in het geheugen (getest met echte ffmpeg-video)
- [x] `W16.08` Meerdere modaliteiten combineren in één vraag; multimodale output (beeld, audio, bestanden) — meerdere bijlagen per bericht (uploads; ook afbeeldingen via de OpenAI-API) worden vóór de cyclus verwerkt en samen in de prompt gezet; uitvoer: beeld (`/beeld`, markdown-afbeelding), spraak (TTS), bestanden (tools); event `chat.attachments.processed`

## W17 — Kwaliteit en performance
Bron: [[waves/W17-kwaliteit-performance]] · status: `open`
- [ ] `W17.01` Record/replay voor tooltests (simulatie vs live)
- [ ] `W17.02` Ablation tests per component; overbodige complexiteit verwijderen — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7)
- [ ] `W17.03` Shadow execution van nieuwe configs
- [ ] `W17.04` Metamorfische + counterfactual tests
- [ ] `W17.05` A/B van prompts, modellen, strategieën
- [ ] `W17.06` Fault injection (crash, volle schijf, verbinding weg, trage tool, kapotte uitvoer, OOM) — **🔒 GROTE TEST: alleen op commando** (zie [[01-REGELS]] regel 7) (kleine fault-tests per component mogen wel)
- [ ] `W17.07` Positionele contexttests per model
- [ ] `W17.08` Speculative decoding (alleen bij gemeten winst), chunked prefill/continuous batching bij eigen backend, logprob-diagnostiek
- [ ] `W17.09` Eigen llama.cpp-bindings indien nodig (KV, logits, LoRA-hotswap)
- [ ] `W17.10` Eigen GPU-kernels, alleen waar profiling (Nsight Systems/Compute) een hotspot aanwijst: gefuseerde kernels voor neurale layers, GPU-scoring voor vector search/rerank, batch-embedding-pre/postprocessing. Prototype in Triton (Python), productie in CUDA C++ aangeroepen vanuit Rust (cudarc) of als PyTorch-extensie. Elke kernel: correctheidstest t.o.v. referentie + benchmark; alleen houden bij gemeten winst. Twee builds: sm_120 (5060 Ti) en sm_61 (1060)
- [ ] `W17.11` Semantische antwoordcache (strikte checks, nooit voor acties), cache-stampede-preventie
- [ ] `W17.12` Incremental recomputation, common subexpression elimination, value-of-information planning, cost-based plan selection (kosten uit eigen metingen)
- [ ] `W17.13` Programmatic tool calling
- [ ] `W17.14` Browser automation, git/repo-tools, computer use, repo-niveau codebegrip
- [ ] `W17.15` **Experiment (gebruiker wil testen):** quantum-inspired tensor-netwerk-compressie (MPO, CompactifAI-achtig: SVD per gewichtenmatrix + korte "healing"-training) — **alleen op eigen neurale layers en hulpmodellen (router/extractie), NOOIT op het redeneermodel** (besluit 2026-11-01: redeneren gaat aantoonbaar omlaag). Vergelijken met gewone quantisatie op kwaliteit × snelheid × VRAM
- [ ] `W17.16` Visuele UI-controle (screenshot vs referentie)
- [ ] `W17.17` Optionele cloud-escalatie (alleen als gebruiker aanzet)

## W18 — Frontier
Bron: [[waves/W18-frontier]] · status: `open`
- [ ] `W18.01` Neural memory module (differentieerbaar associatief geheugen)
- [ ] `W18.02` Continual learning zonder catastrophic forgetting (adapters, replay, regularisatie)
- [ ] `W18.03` Causaal/counterfactual redeneren en tracking
- [ ] `W18.04` Wereldmodel-simulaties
- [ ] `W18.05` Vortex schrijft, test en registreert eigen tools
- [ ] `W18.06` Self-improving prompts en tools
- [ ] `W18.07` Multi-agent gedeeld + privé geheugen
- [ ] `W18.08` Quantum-tool: Qiskit, circuits lokaal simuleren (~30 qubits haalbaar) + IBM Quantum Open Plan (10 min per 28 dagen) voor echte QPU-runs in research

## W19 — Ethics & security layer
Bron: [[waves/W19-ethics-security]] · status: `geparkeerd`
- Geparkeerd. Trigger: **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"** — zie [[GEPARKEERD]].

## W20 — Trading
Bron: [[waves/W20-trading]] · status: `geparkeerd`
- Geparkeerd. Trigger: **"TRADING NU MAKEN"** — zie [[GEPARKEERD]].
