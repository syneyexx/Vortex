---
type: besluitenlog
updated: 2026-11-02
---
# Besluitenlog
Nieuwste onderaan. Formaat: datum · besluit · waarom.

| Datum | Besluit | Waarom |
| --- | --- | --- |
| 2026-10-31 | Naam: **Vortex** | Info in → verwerken → eruit; alleen persoonlijk gebruik |
| 2026-10-31 | Talen: **Rust** core, **Python** ML/data, **TypeScript** frontend (Tauri) | Snel + zuinig + veilig parallel; ML-ecosysteem zit in Python |
| 2026-10-31 | Modellen via abstractielaag; start met llama.cpp `llama-server` (OpenAI-compatibel), later eigen bindings | Direct bouwen aan het slimme deel; later KV-cache/logits/LoRA-hotswap |
| 2026-10-31 | Opslag: **SQLite** (bron van waarheid, FTS5, graph-edges) + **LanceDB** (vectoren) + **moka** cache, achter `MemoryStore`-trait | Embedded, betrouwbaar, verwisselbaar; geen netwerkhops |
| 2026-10-31 | Eén ID per herinnering; alle indexen afgeleid | Verwijderen/samenvoegen werkt overal door |
| 2026-10-31 | SSD = warm (code, actieve modellen, DB's, index); HDD = koud (ruwe data, volledige teksten, archief, backups) | 250 GB SSD; index op SSD, inhoud op HDD → terabytes zonder trage zoekacties |
| 2026-10-31 | Geheugen schaalt naar 3–4 TB via tiering | Gebruikerswens |
| 2026-10-31 | Reasoning via debatteam (Planner, Solvers, Fact-checker, Critic, Specialist, Judge, Synthesizer), dynamisch | Betere antwoorden; diversiteit en objectieve checks |
| 2026-10-31 | Antwoord pas tonen nadat Judge vertrouwt | Gebruikerskeuze |
| 2026-10-31 | Offline-first, automatische webcheck bij verbinding | Gebruikerskeuze |
| 2026-10-31 | Conflict-team lost botsingen direct op en corrigeert geheugen | Gebruikerswens |
| 2026-10-31 | Framework = "50% van het brein": eigen neurale layers + LoRA-vliegwiel | Niet gelimiteerd aan modelgewichten |
| 2026-10-31 | Vanaf dag 1 alle traces + feedback loggen | Trainingsdata is achteraf niet terug te halen |
| 2026-10-31 | System prompt + rol-prompts bewerkbaar in Instellingen, live vanaf volgend bericht; lopende run houdt versie tenzij "nu toepassen"; versiegeschiedenis | Gebruikerswens + consistentie |
| 2026-10-31 | GUI 1-op-1 volgens `design/reference-chat.png`; cijfers daarin zijn mockdata; vortex als geanimeerde WebGL-shader | Gebruikerswens |
| 2026-10-31 | Eén taak tegelijk; modellen wisselen per taak | Gebruikerswens + 16 GB VRAM |
| 2026-10-31 | Externe projecten (OpenBB, Fincept, Godel, Nautilus, Backtrader) alleen inspiratie; eigen connectors op endpoints | Geen afhankelijkheid van andermans roadmap |
| 2026-10-31 | Skills: importeren/omzetten van Claude Skills + Cursor rules, zelf gebruiken en schrijven | Gebruikerswens |
| 2026-10-31 | Wereldmonitor (nieuws/events, geen visuals) zodat Vortex de wereld leert begrijpen | Gebruikerswens |
| 2026-10-31 | Grote spelers per sector onderzoeken en ervan leren (publieke professionele bronnen) | Gebruikerswens |
| 2026-10-31 | Trading geparkeerd tot "TRADING NU MAKEN" | Pas als de rest optimaal werkt |
| 2026-10-31 | Ethics & security layer helemaal aan het eind, bij "WE GAAN DE ETHICS EN SECURITY LAYER MAKEN" | In één keer goed aanpakken |
| 2026-10-31 | Werkregels 1–6 vastgelegd | Zie [[01-REGELS]] |
| 2026-10-31 | Bouwplan in waves; VAULT als tweede brein | Gebruikerswens |
| 2026-11-01 | Geen eigen programmeertaal; Rust blijft. Snelheid uit GPU-kant: eigen kernels (Triton/CUDA) alleen bij gemeten hotspot (W17) | Rust zit al op native-plafond; bottleneck is GPU/bandbreedte |
| 2026-11-01 | CUDA-toolchain moet sm_61 (1060) én sm_120 (5060 Ti) dekken; CUDA 13 valt af voor 1060 | Pascal-ondersteuning gestopt in CUDA 13 |
| 2026-11-01 | Model-efficiëntie: MoE + expert-cache, NVFP4 testen, KV-quant, quant-shootout op eigen eval, 1060 int8 | Decode is bandbreedte-gebonden; zie 05-MODELLEN |
| 2026-11-01 | Geen echte quantum entanglement in Vortex (klassieke hardware kan dat fysiek niet); wel quantum-inspired technieken + quantum-tool als W18-experimenten | Onderzoek: geen aangetoonde QML-winst op klassieke data; tensor-netwerken en simulatie wel bruikbaar |
| 2026-11-01 | Een experimenteel neven-idee is afgesplitst als volledig apart project (eigen map, eigen prompt); valt buiten Vortex | Strikte scheiding op verzoek gebruiker |
| 2026-11-01 | Tensor-netwerk-compressie nooit op het redeneermodel; alleen testen op eigen neurale layers/hulpmodellen | Redeneren lijdt het hardst onder lossy compressie; redeneren is kern van Vortex |
| 2026-10-31 | Centrale Dirigent wijst alle taken toe (P0–P3, admission, preemption, model-affiniteit, pipelining); CCD0 interactief / CCD1 achtergrond; RAM-budget per modus; nul-lekbeleid met soak-tests en performance-gates | Maximale snelheid en stabiliteit op 5950X + 5060 Ti + 1060 + 32 GB |
| 2026-10-31 | Eigen research-team naast chat-pipeline (Lead, Scouts, Readers, Extractor, Bronbeoordelaar, Analysts, Fact-checker, Skepticus, Judge, Writer, Archivist); vondsten direct omgezet naar getypeerde data | Research optimaal + meteen bruikbare data |
| 2026-10-31 | Eigen webcrawler-team (Scout, Crawler, Reader, Ranker, Curator) voor research en opzoeken | Dieper en gerichter dan alleen zoekresultaten; W10 basis, W14 headless + recrawl |
| 2026-11-02 | Project staat in GitHub-repo `syneyexx/Vortex`; repo-root = `vortex/` uit [[10-ARCHITECTUUR]], VAULT in de repo | Eén bron, versiebeheer, werken vanaf PC én web |
| 2026-11-02 | Elk wave-item heeft een vast ID (`W05.07`); [[TODO]] wordt gegenereerd uit de waves door `tools/vault/vault.py` | Eén bron van waarheid, geen dubbele lijsten die uit elkaar lopen |
| 2026-11-02 | Navigatie voor 500K–1M regels: `CARD.md` per crate, code-tags `VX:<ID>`, [[20-CODEMAP]], regels N1–N9 in [[12-NAVIGATIE]] | Claude hoeft nooit het hele project te lezen |
| 2026-11-02 | Dubbel item "Research-pagina in GUI" uit W10 verwijderd; wave-verwijzingen in module-frontmatter genormaliseerd naar `W0n` | Opruimen |
| 2026-11-02 | Repo op NVMe-SSD (`C:\dev\Vortex`), grote data op HDD's; kern + GUI native Windows, training native vs WSL2 testen in `W00.06` | Antwoord gebruiker 1–3; Claude kiest platform na meting |
| 2026-11-02 | Extra crates `vortex-types` en `vortex-crawler`; `proto/` als bron van alle procescontracten, kaart in [[15-CONTRACTEN]] | Geen afhankelijkheidscycli; crawler eigen ritme; één contractbron |
| 2026-11-02 | W01 gesplitst: W01 runtime kern (+ simpele GPU-scheduler) en nieuwe [[waves/W04-dirigent-performance]]; latere waves één nummer op (oud W04 → W05 … W19 → W20) | W01 was te groot; Dirigent pas nodig als er meerdere soorten werk draaien |
| 2026-11-02 | Verticale plak: W00–W03 leveren chat end-to-end | Vroeg iets werkends; gates blijven gelden |
| 2026-11-02 | Geen vaste TTFT-grens van 1 s; meten in W00 en dan vastzetten; Deep toont direct voortgang, antwoord na Judge | Gebruiker: 1 s niet haalbaar; Judge-gate blijft |
| 2026-11-02 | Debatteam: snelheid meten in W09, Solvers zo nodig parallel via slots | Eén hoofd-GPU |
| 2026-11-02 | `vortex` CLI (`W00.15`) en `justfile` (`W00.14`) | Debuggen + Claude kan zelf status/eval draaien |
| 2026-11-02 | Geen rookproef-eval vanaf W00 ("testen pas als laatst"); reikwijdte voor unit-tests nog open in [[OPEN-VRAGEN]] | Gebruikerskeuze |
| 2026-11-02 | Regel 7: kleine tests tussendoor bij elk item; grote tests (24 u soak, weekruns, volledige eval/performance-suites, ablations, fault-injection, TB-simulatie) alleen op commando van de gebruiker | Antwoord op vraag 13: fouten vroeg vangen zonder lange runs |
| 2026-11-02 | Doorbouwen aan volgende waves terwijl PC-verificatie van W00–W02 openstaat; gates blijven pas groen na verificatie op de PC | Gebruiker: "ga maar verder" |
| 2026-11-02 | GUI-transport: WebSocket met event-JSON (`/api/events?from_seq`) + REST, i.p.v. gRPC-web | Eén stroom = het event log zelf; hervatten op `seq`; geen extra codegen in de browser |
| 2026-11-02 | Antwoordtekst wordt tijdens streamen alleen als gecoalesceerde deltas in het event log bewaard; het bericht krijgt zijn tekst bij afronden; lezen tijdens streamen bouwt op uit deltas | Eén bron, crash-/herlaadveilig |
| 2026-11-02 | Alle GPU-werk loopt via de Dirigent (`RunContext::gpu` → `Dirigent`); de simpele slot-scheduler blijft alleen als bouwsteen | Eén plek voor prioriteit, preemption, pauzeren en kostenmodel |
| 2026-11-02 | Windows-code wordt in de cloud getypecheckt met `cargo check --target x86_64-pc-windows-gnu` (zig als C-compiler); gedrag verifiëren op de PC | Compileerfouten vóór de PC-sessie vangen |
| 2026-11-02 | Geheugenvectoren voorlopig in eigen int8-index (in SQLite) achter `VectorIndex`-trait i.p.v. LanceDB; bevestiging gevraagd in [[OPEN-VRAGEN]] | Lichte build, transactioneel met de herinnering, snel genoeg tot miljoenen; LanceDB/ANN kan later |
| 2026-11-02 | Geheugen-traits (`Embedder`, `MemLlm`, `Reranker`) in `vortex-memory`; model-adapters in `vortex-context` | Lagenregel: memory en models zitten in dezelfde laag |
| 2026-11-02 | 1060-team = drie llama-server-processen (embed 8081, rerank 8082, router 8083) | llama-server laadt één model per proces |
| 2026-11-02 | Geen inhoud in geheugen-events; belang van episodisch geheugen komt uit de gebruikersvraag | Vergeten moet echt vergeten zijn; modelantwoord mag de policy niet sturen |
| 2026-11-02 | Eval draait via de echte chat-pipeline (`Subject`), in geïsoleerde `__eval_*`-werkruimtes die na afloop vergeten worden | Meet wat de gebruiker krijgt; geen lekkage naar het echte geheugen |
| 2026-11-02 | Vectorzoeken parallel boven 50k vectoren (scoped threads) | Bench: 17,8 → 7,0 ms per 100k op 4 vCPU; houdt p95 < 150 ms richting miljoenen |
| 2026-11-02 | Elke chatbeurt loopt door de cognitieve cyclus (`ChatEngine::run`); snel pad = 1 modelaanroep, diep pad = plan + stappen + kritiek vóór het antwoord | One brain: één pijplijn, events per fase |
| 2026-11-02 | Metacognitie vraagt niet terug op korte invoer; alleen sturing "vraag kort na als het onduidelijk is" | Korte vragen ("traag", "welke prompt?") werden geblokkeerd |
| 2026-11-02 | Router-LM-label alleen bij heuristische zekerheid < 0,65, timeout 1,5 s | Snelheid: meeste beurten geen extra modelaanroep |
| 2026-11-02 | Bijsturen = huidige generatie annuleren, gedeeltelijk antwoord + nieuwe instructie meegeven, verder streamen in hetzelfde bericht | Geen nieuwe run; gebruiker ziet één doorlopend antwoord |
| 2026-11-02 | Idempotentie per actie-ID in een eigen `tool_ledger` (niet de run-gebonden `effects`), sleutel = `<run>:<call-id>` in de chat | Werkt ook buiten runs (handmatig, eval); replay i.p.v. tweede effect |
| 2026-11-02 | Lokale schrijftools altijd via een transactie (staging → postcondities → atomair); dry-run = preview | Toolsucces ≠ doel bereikt; nooit half toegepast |
| 2026-11-02 | Tool discovery: vaste kernset met schema, rest via `describe_tool` + `call_tool` | Prompt klein bij veel MCP-/API-tools; prefix stabiel voor prompt caching |
| 2026-11-02 | Skills op schijf in `config/skills/` in Claude-compatibel formaat (+ extra frontmatter); Cursor/AGENTS.md via converters | Uitwisselbaar met bestaande ecosystemen; hot-reload via de map |
| 2026-11-02 | Capability handle standaard volledig open | Geen remmen tot "WE GAAN DE ETHICS EN SECURITY LAYER MAKEN" (W19) |
| 2026-11-02 | Debatteam draait vóór het streamen; het gestreamde antwoord is de Synthesizer (met dezelfde bijstuur-/toolmachinerie) | Judge-gate: niets tonen vóór vertrouwen; één streampad |
| 2026-11-02 | Kandidaatkeuze op verifier-score (objectieve checks > critic > claims), niet op stemmen | "Overeenstemming bewijst niets" |
| 2026-11-02 | Online = zoekbron ingesteld én bereikbaar (60 s cache); anders scope "lokaal" | Offline-first, geen stille webcalls |
| 2026-11-02 | Conflictbeslissingen via `apply_resolution` met historie-tabel; rollback zet waarden terug en heropent | Alles gelogd en terug te draaien |
| 2026-11-02 | `profile.dev`: debug = line-tables-only, dependencies zonder debug-info | Cloud-schijf liep vol (16 GB target); backtraces houden regelnummers |
| 2026-11-02 | Worker schrijft per document JSONL naar de koude schijf; de kern leest en schrijft naar het geheugen | Grote documenten niet over gRPC; ruwe/tussendata op HDD, kennis op SSD |
| 2026-11-02 | Ingest gebruikt de bestaande geheugen-schrijfpijplijn (W05) per sectie met locator + document-hash | Eén pijplijn voor chunken/claims/dedup/conflicten; incrementeel per bestand |
| 2026-11-02 | Research-records krijgen een vaste bron-hash (hash van de paginatekst) | Anders zette elk nieuw record uit dezelfde bron de vorige op verouderd |
| 2026-11-02 | Parquet-export via Python + pyarrow (sandbox), CSV/JSONL in Rust | Geen zware Parquet-crate in de kern |
| 2026-11-02 | Research-modus: Research-knop/deep → deep; korte eenvoudige vraag → quick; anders standaard | "Metacognitie kiest; Research-knop overrulet" |
| 2026-11-02 | Eigen pure-Rust ONNX-runtime voor de MLP-layers i.p.v. `ort` | Geen native onnxruntime-DLL/CUDA-koppeling nodig; MLP's draaien in µs op de CPU; grotere modellen blijven op llama.cpp |
| 2026-11-02 | Haak `LayerScorer` in vortex-core; featurevectoren in de consumerende crate | Elke crate kan een layer gebruiken zonder van vortex-neural af te hangen (lagenregel) |
| 2026-11-02 | Layer-trainer: PyTorch als aanwezig, anders numpy; ONNX-graaf zelf gebouwd | Werkt overal (ook zonder torch); zelfde architectuur en export |
| 2026-11-02 | Promotie alleen bij ≥ 2 pt winst t.o.v. heuristiek én actieve versie; actieve versie die verliest → automatisch terug | "Elk net wint aantoonbaar van zijn heuristiek"; veilige rollback |
| 2026-11-02 | `LayerRegistry` schrijft events via een eigen thread (nooit synchroon) | `decide` mag nooit blokkeren op de database (deadlock gevonden binnen een transactie) |
| 2026-11-02 | Geldigheid van feiten = die van de edge, anders van de herinnering | Conflictbeslissingen en rollbacks werken zonder extra synchronisatie door in de graaf |
| 2026-11-02 | Afgeleide feiten worden per redeneer-ronde volledig opnieuw berekend | Altijd consistent met de basis; weg = weg |
| 2026-11-02 | Koude laag: blokken van 64 herinneringen (zstd 9) + centroid; treffers worden warm teruggehaald | Snel op HDD (weinig seeks), klein, SSD-cache voor herhaling |
| 2026-11-02 | Kwantum-achtig kansmodel blijft een experiment; Bayes blijft standaard | A/B: kwantum alleen beter bij gekopieerde bronnen — mogelijk later als correctie voor afhankelijke bronnen |
| 2026-11-02 | Adapters als GGUF-LoRA in llama-server (geladen met schaal 0, hot-swap via `/lora-adapters`) i.p.v. gemergde modellen per versie | Geen herstart of extra VRAM per versie; terugschakelen in milliseconden |
| 2026-11-02 | DPO-labels apart: correctness (objectieve check/Judge) vs preference (gebruiker) | Voorkeur ≠ feitelijk correct (zie modules/training) |
| 2026-11-02 | Poortwachter: winst ≥ 2 pt op een suite, geen regressie > 2 pt, schaduwrun ≥ 50 % | "Een adapter wint op de eval zonder regressie elders" |
| 2026-11-02 | Training alleen in idle; bij gebruik worker annuleren + checkpoint, later hervatten | GPU direct vrij voor de gebruiker |

