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

**Actieve wave:** PC-verificatie W00–W04 · cloud: volgende = W05 (geheugen)
