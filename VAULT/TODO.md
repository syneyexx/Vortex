---
type: todo
generated: true
---
# TODO — alle taken van Vortex (gegenereerd)

> **Niet met de hand bewerken.** Bron = `waves/*.md`. Afvinken doe je in de wave-file,
> daarna `python tools/vault/vault.py todo`. Legenda: `[ ]` open · `[~]` bezig · `[x]` af.
> Functionaliteiten per domein (wat Vortex *kan*): [[FUNCTIES]].

**Totaal: 91/235 af.**

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
| [[waves/W09-debatteam\|W09 — Debatteam]] | open | 0 | 11 |
| [[waves/W10-research-ingest\|W10 — Research en ingest]] | open | 0 | 14 |
| [[waves/W11-neurale-layers\|W11 — Neurale layers]] | open | 0 | 9 |
| [[waves/W12-geheugen-gevorderd\|W12 — Geheugen gevorderd]] | open | 0 | 10 |
| [[waves/W13-training-vliegwiel\|W13 — Training en vliegwiel]] | open | 0 | 9 |
| [[waves/W14-wereldkennis\|W14 — Wereldkennis]] | open | 0 | 9 |
| [[waves/W15-autonomie\|W15 — Autonomie]] | open | 0 | 10 |
| [[waves/W16-multimodaal\|W16 — Multimodaal]] | open | 0 | 8 |
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
Bron: [[waves/W09-debatteam]] · status: `open`
- [ ] `W09.01` Rollen met prompts in `config/roles/` (bewerkbaar in Instellingen)
- [ ] `W09.02` Dynamische teamsamenstelling door metacognitie
- [ ] `W09.03` Solvers met diversiteit (hoek, temperatuur, evt. model)
- [ ] `W09.04` Fact-checker: claims splitsen (1060), bewijs lokaal; connectiviteitscheck → webcheck online, label offline
- [ ] `W09.05` Critic, Specialist (code-reviewer draait tests), verifier-guided candidate selection
- [ ] `W09.06` Judge-gate: antwoord pas na vertrouwen; max rondes; anders eerlijk onzeker/vraag
- [ ] `W09.07` Synthesizer in gebruikersstijl
- [ ] `W09.08` Bewijsoverzicht per conclusie
- [ ] `W09.09` Conflict-team (2 onderzoekers, bronbeoordelaar, judge, memory writer), voorgrond/achtergrond, geldigheidsperiodes, "betwist", rollback
- [ ] `W09.10` Agent collaboration-paneel + Verdict-balk live; voortgang gescheiden van antwoord
- [ ] `W09.11` Parallel onderzoek / subagents met geïsoleerde contexten, overdracht via verwijzingen

## W10 — Research en ingest
Bron: [[waves/W10-research-ingest]] · status: `open`
- [ ] `W10.01` Python-worker ingest-service (gRPC), voortgang-events
- [ ] `W10.02` Parsers: EPUB, PDF (tekst), DOCX, Markdown, HTML, code
- [ ] `W10.03` Structuur-bewust chunken, contextual chunk descriptions, content-hashes, incrementeel
- [ ] `W10.04` Entiteit/claim-extractie op 1060, graph-koppeling, dedup, conflictcheck
- [ ] `W10.05` Hiërarchische samenvattingen (sectie → hoofdstuk → boek)
- [ ] `W10.06` Onderwerp-research: plan, deelvragen, zoeken, lezen, iteratieve retrieval, adaptief bijsturen, ontbrekend bewijs zoeken, kruiscontrole
- [ ] `W10.07` Webcrawler-team: Scout, Crawler (HTTP), Reader/Extractor, Ranker, Curator; focused crawling met budgetten; frontier in SQLite; ruwe HTML op HDD; per-domein tempo + robots.txt; cache
- [ ] `W10.08` Research-team volgens [[modules/research-team]]: alle 11 rollen, loop (8 stappen), drie modi, Judge-gate per deelvraag
- [ ] `W10.09` Extractor-schema's (claim, getal, entiteit, relatie, event, tabel, citaat, bron) + validatie + direct naar geheugen via Archivist
- [ ] `W10.10` Parallel: crawlers (CPU) + readers/extractor (1060) terwijl hoofdmodel analyseert
- [ ] `W10.11` Research-pagina: live records, deelvragen, bronnen, bewijsoverzicht, export CSV/Parquet
- [ ] `W10.12` Brongebonden antwoordopbouw (passage ↔ claim), rapporten/tabellen als artefact
- [ ] `W10.13` Global corpus synthesis
- [ ] `W10.14` Ruwe data op HDD, kennis op SSD

## W11 — Neurale layers
Bron: [[waves/W11-neurale-layers]] · status: `open`
- [ ] `W11.01` Feature-extractie uit event log → datasets per layer (versie, decontaminatie)
- [ ] `W11.02` Bootstrap-labeling met LM
- [ ] `W11.03` Trainingsscripts (PyTorch) per layer + ONNX-export
- [ ] `W11.04` `vortex-neural`: ONNX laden via `ort` (1060/CPU), hot-swap van versies
- [ ] `W11.05` Layers: router-net, importance scorer, entity matcher, salience-net, usefulness-predictor, skill selector, decay-predictor
- [ ] `W11.06` Reranker fine-tune op eigen feedback
- [ ] `W11.07` Schaduwmodus + automatische promotie/rollback via eval
- [ ] `W11.08` Idle-training (koppeling scheduler)
- [ ] `W11.09` Dashboard: versies en scores per layer

## W12 — Geheugen gevorderd
Bron: [[waves/W12-geheugen-gevorderd]] · status: `open`
- [ ] `W12.01` Procedural memory (skills + werkwijzen, succesrate, versies)
- [ ] `W12.02` Meta-memory (wat bekend, hoe zeker, gaten) → leeragenda
- [ ] `W12.03` Dreaming in idle: consolidatie, clustering/merging, decay/pruning, hiërarchische tijd-samenvattingen, reflectie, contradiction resolution, confidence-updates
- [ ] `W12.04` Temporal knowledge graph: validity from/to, historie
- [ ] `W12.05` Graph-redeneren (afleidingen, consistentie, regels)
- [ ] `W12.06` Zelfmodel uit eval-resultaten → router/metacognitie
- [ ] `W12.07` Tiering SSD/HDD: zstd-blokken, ophalen + SSD-cache, prefetch, decay-gestuurd; test met synthetische TB-schaal
- [ ] `W12.08` Multi-modaal geheugen (afbeeldingen, bestanden als referentie met embeddings)
- [ ] `W12.09` **Experiment (gebruiker wil testen):** quantum-achtige kansmodellen (niet-klassieke kansregels, volgorde-effecten, interferentie tussen hypotheses) voor onzekerheid, tegenstrijdige bronnen en contradiction resolution; A/B tegen gewone Bayesiaanse aanpak op conflict-eval
- [ ] `W12.10` Memory-based few-shot (voorbeelden uit geheugen in context)

## W13 — Training en vliegwiel
Bron: [[waves/W13-training-vliegwiel]] · status: `open`
- [ ] `W13.01` Dataset-builders: SFT (distillatie team-antwoorden), DPO (gekozen/verworpen, voorkeur vs correct), Q&A/redeneervoorbeelden uit ebooks/datasets
- [ ] `W13.02` QLoRA-pipeline (Unsloth/PEFT) op Qwen3.5-9B safetensors; mergen/converteren → GGUF
- [ ] `W13.03` Adapter-registry: basismodel, datasetversie, instellingen, scores, regressies
- [ ] `W13.04` Adapters los laden/hot-swappen waar backend het kan
- [ ] `W13.05` Verifier/reward model + process reward model (tussenstappen)
- [ ] `W13.06` Confidence calibrator + kalibratiemeting (gescheiden dev/test)
- [ ] `W13.07` Poortwachter: eval + schaduwrun → promoveren of terug
- [ ] `W13.08` Training alleen in idle, GPU vrijgeven bij gebruik
- [ ] `W13.09` Training/evals-pagina in GUI

## W14 — Wereldkennis
Bron: [[waves/W14-wereldkennis]] · status: `open`
- [ ] `W14.01` Connectors: GDELT, RSS (bronnenlijst meerdere landen/richtingen), Wikipedia Current Events, Wikidata, ReliefWeb, USGS, NASA EONET, World Bank, Eurostat, CBS, arXiv, Hacker News — rate limits + voorwaarden per bron
- [ ] `W14.02` Gepland ophalen + inhaal-modus
- [ ] `W14.03` Crawler: headless browser voor JS-pagina's, recrawl-schema voor gevolgde sites, Ranker als neurale layer (data uit W10)
- [ ] `W14.04` Event-clustering (artikelen → één event), gemeld vs bevestigd
- [ ] `W14.05` Temporal KG-integratie, bronbias
- [ ] `W14.06` Causale hypotheses + voorspellingen + later toetsen (kalibratie wereldmodel)
- [ ] `W14.07` Dag/week/maand-samenvattingen per thema/regio
- [ ] `W14.08` Spelers per sector: bronnen verzamelen, beslissingen/principes/uitkomsten extraheren, mislukkingen, voorspellingen toetsen
- [ ] `W14.09` HF-datasets: streamen, schema, mapping-voorstel + bevestiging, filter, dedup (MinHash), decontaminatie, licentie, bestemming kiezen, catalogus

## W15 — Autonomie
Bron: [[waves/W15-autonomie]] · status: `open`
- [ ] `W15.01` Idle-detectie + GPU direct vrijgeven bij gebruik
- [ ] `W15.02` Leeragenda zelf uitvoeren in idle
- [ ] `W15.03` Taakqueue: prioriteit, afhankelijkheden, long-running projecten, multi-session continuity
- [ ] `W15.04` Planning en triggers (tijd, gebeurtenis)
- [ ] `W15.05` Proactieve monitoring en suggesties
- [ ] `W15.06` Dagelijkse briefing
- [ ] `W15.07` Lokale OpenAI-compatibele API (Cursor/eigen projecten)
- [ ] `W15.08` Telefoontoegang via Tailscale + mobiele weergave
- [ ] `W15.09` Voortgangsdashboard "hoe slim is Vortex"
- [ ] `W15.10` Automatische backups + restore-test

## W16 — Multimodaal
Bron: [[waves/W16-multimodaal]] · status: `open`
- [ ] `W16.01` Visie (Qwen3.5-9B): afbeeldingen, screenshots, grafieken; ROI-verwerking (overzicht → uitsnede)
- [ ] `W16.02` OCR (GLM-OCR / PaddleOCR-VL) met onzekerheid doorgeven
- [ ] `W16.03` Documentverwerking: PDF-lay-out, DOCX, spreadsheets; tabellen met structuurbehoud
- [ ] `W16.04` Visuele documentretrieval (ColPali-achtig) + coördinaat-bronverwijzingen
- [ ] `W16.05` Spraak in (Whisper-klasse, CPU) + uit (TTS), onderbreekbaar, voice-modus in GUI
- [ ] `W16.06` Beeldgeneratie/-bewerking (na ontladen LLM)
- [ ] `W16.07` Video: adaptieve frameselectie + tijdsuitlijning met audio/transcript
- [ ] `W16.08` Meerdere modaliteiten combineren in één vraag; multimodale output (beeld, audio, bestanden)

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
