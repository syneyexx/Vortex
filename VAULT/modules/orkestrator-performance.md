---
type: module
crate: vortex-core (orchestrator) + alle crates
waves: [W01, W04, W02, W06, W11, W17]
updated: 2026-10-31
---
# Orkestrator en performance

Eén **Dirigent** in de Rust-kern wijst alle taken toe aan de hardware. Niets start buiten de Dirigent om. Doel: alles zo snel als de hardware toelaat, constant strak, nul geheugenlekken.

## Hardware-budget
| Middel | Capaciteit | Vaste verdeling |
| --- | --- | --- |
| CPU Ryzen 9 5950X | 16 cores / 32 threads, 2 CCD's van 8 cores | **CCD0 (core 0–7): interactief** — kern, chat, inference-host, GUI-server · **CCD1 (core 8–15): achtergrond** — crawlers, parsers, Python-worker, ONNX, dreaming. In Deep-modus krijgt MoE-offload CCD1 en krimpt de achtergrond |
| RTX 5060 Ti 16 GB | 1 hoofdmodel tegelijk | gewichten + KV-cache + buffers via admission control; ~0,5–1 GB marge |
| GTX 1060 6 GB | vast team | embedding + reranker + router-model; vaste VRAM-reservering |
| RAM 32 GB | budget per modus | zie tabel hieronder |
| SSD | snel, random OK | warm geheugen, modellen, DB's |
| HDD | traag bij random | **één I/O-wachtrij**, alleen sequentieel, gesorteerd lezen |

### RAM-budget per modus (startwaarden, bijstellen na meting)
| Post | Normaal | Deep (MoE-offload) |
| --- | --- | --- |
| Windows + overige apps | 6 GB | 6 GB |
| Vortex-kern + GUI | 1,5 GB | 1,5 GB |
| Python-worker | 3 GB | 1,5 GB |
| DB-caches (SQLite, LanceDB) + moka | 3 GB | 2 GB |
| Model-RAM-cache (sneller wisselen) | 12 GB | 0 GB |
| MoE-experts in RAM | 0 GB | ~14 GB |
| Reserve | ~6 GB | ~7 GB |
Harde caps per proces (Windows Job Objects); boven de cap: netjes herstarten na checkpoint.

## De Dirigent (orkestrator)
**Taken melden zich aan met:** soort, prioriteitsklasse, benodigde middelen (hoofd-GPU/VRAM, 1060, CPU-cores, RAM, SSD/HDD-I/O, netwerk), deadline, welk model, annuleerbaar/hervatbaar.

**Prioriteitsklassen**
- P0 — interactief: chat, stopknop, GUI (altijd direct)
- P1 — gebruiker wacht: research die jij startte, tool die de chat blokkeert
- P2 — achtergrond met waarde: ingest, crawlen, conflict-team
- P3 — idle: dreaming, training, leeragenda, herindexeren

**Mechanismen**
- **Admission control** per middel (bin-packing); past het niet → wachtrij, nooit OOM.
- **Preemption**: P0 komt binnen → P2/P3 pauzeren op een veilig punt (tokengrens, batchgrens, checkpoint) en geven GPU/cores vrij.
- **Priority inheritance**: een P2-taak waar de chat op wacht wordt tijdelijk P0.
- **Model-affiniteit**: taken voor hetzelfde model bundelen → minimaal wisselen; volgende model alvast laden als de route voorspelbaar is.
- **Pipelining**: crawlen (CPU) ‖ lezen/extraheren (1060) ‖ analyseren (5060 Ti) lopen tegelijk via begrensde kanalen.
- **Bulkheads**: vaste maxima per soort werk; een vastgelopen parser bezet nooit alles.
- **Backpressure**: volle wachtrij → producent vertraagt.
- **Kostenmodel**: elke taak meet duur, VRAM, RAM, CPU; de Dirigent leert die kosten (later een neurale layer) en plant daarop.
- **Drukmeters**: VRAM, RAM, schijf, CPU-belasting, GPU-temperatuur/clocks; bij druk eerst P3 en P2 terugschroeven.
- **Idle-detectie**: gebruiker actief (game, andere zware app) → alles behalve P0 pauzeert, GPU vrij.

**Twee tokio-runtimes:** interactief (CCD0, klein, lage latency) en achtergrond (CCD1); blocking pool begrensd. Python-worker en ingest-processen met CPU-affiniteit op CCD1 en lagere Windows-prioriteit; kern en llama-server hoofd-GPU op hogere prioriteit.

## Nul geheugenlekken
**Rust-kern**
- Elk kanaal begrensd; elke cache met grootte-cap (moka weigher); geen onbegrensde Vecs/Maps in langlevende structs.
- Geen `Rc`/`Arc`-cycli (Weak voor terugverwijzingen); request-scoped data vrijgegeven aan einde run.
- mimalloc of jemalloc als allocator (minder fragmentatie).
- Resource ownership: elk proces, handle, GPU-slot heeft een eigenaar die opruimt (ook bij panic).
- tokio-console om hangende/lekkende tasks te zien.

**Python-worker**
- Werkprocessen worden **gerecycled na N taken** (max-tasks-per-child) → lekken in libraries kunnen niet opstapelen.
- GPU-geheugen expliciet vrijgeven na training/inference; elke job in eigen subprocess bij zware libs.

**GUI**
- Gevirtualiseerde berichtlijst (alleen zichtbare berichten in DOM); oude streams worden samengevoegd.
- WebGL-resources en event listeners netjes opruimen; shader pauzeert als venster verborgen is.

**Bewaking**
- Geheugen-telemetrie per proces in tracing; GPU-VRAM vergeleken met verwacht budget.
- **Soak-test** 🔒 (alleen op commando, regel 7): 24 uur draaien met gesimuleerd gebruik; RSS-trend moet vlak zijn, anders gate rood.
- Heap-profiling (dhat/heaptrack-achtig) bij elke afwijking.

## Snelheidstechnieken
- Zero-copy waar kan (`bytes`, `Arc<str>`), embeddings in batches, single-flight.
- SQLite: WAL, één schrijfthread + leespool, prepared statements, juiste indexen. LanceDB: geschikte ANN-index, mmap.
- HDD: één wachtrij, gesorteerd, grote blokken, zstd; prefetch naar SSD.
- FlashAttention + KV-quant; context zo kort als kan (assembler); prompt caching.
- GUI: 60 fps tijdens streamen door event coalescing.

## Prestatiedoelen (startwaarden — eerst meten in W00/W02, dan vastzetten)
| Meting | Doel |
| --- | --- |
| Eerste token Quick (model geladen, korte prompt) | meten in W00, dan vastzetten — geen vaste 1 s (besluit 2026-11-02) |
| Deep/Research: voortgang zichtbaar na verzenden | direct (agent-paneel); antwoord pas na Judge |
| Retrieval p95 (warm, SSD) | < 150 ms |
| Stopknop → werk echt gestopt | < 500 ms |
| GUI tijdens streamen | 60 fps |
| CPU in rust (idle, geen P3) | < 2% |
| RSS-groei over 24 h soak | ~0 |
Elke doelstelling wordt een **performance-test**; achteruitgang = gate rood. De volledige performance-suite draait 🔒 op commando (regel 7); losse snelle metingen (bv. stoplatency-test) mogen tussendoor.

## Profiling
Tracing-spans → flamegraphs (Tracy of Superluminal op Windows, cargo-flamegraph), criterion-benchmarks voor hete paden, GPU-metingen per stap.

## Windows-tips
Energieplan hoge prestaties; hardware-accelerated GPU scheduling testen (meten, niet aannemen). Het Windows-bureaublad gebruikt VRAM op de GPU waar je monitor aan zit — meet hoeveel dat op de 5060 Ti kost en neem het mee in het budget.

## Verbindingen
Leest middelenvraag uit toolcontracten en model-registry; schrijft elke beslissing naar het event log; kostenmodel → neurale layer; drukmeters en wachtrijen zichtbaar in statusbalk en Lopend werk-pagina.
