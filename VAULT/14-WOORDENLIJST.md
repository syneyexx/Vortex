---
type: referentie
updated: 2026-11-02
---
# Woordenlijst

| Term | Betekenis | Waar |
| --- | --- | --- |
| **Vortex** | het hele framework (harness) + model = één brein | [[02-VISIE]] |
| **One brain** | elke module aangesloten op event log, workspace, geheugen, metacognitie | [[11-ONE-BRAIN]] |
| **Event log** | append-only log van alles wat gebeurt; bron voor tracing, replay, training | [[modules/runtime-core]] |
| **Outbox** | event + statuswijziging in één SQLite-transactie | [[modules/runtime-core]] |
| **Global Workspace** | gedeeld "schoolbord" per taak: doel, plan, herinneringen, hypotheses | [[modules/cognitie]] |
| **Dirigent** | centrale orkestrator die alle taken aan hardware toewijst (P0–P3) | [[modules/orkestrator-performance]] |
| **P0–P3** | prioriteitsklassen: interactief / gebruiker wacht / achtergrond / idle | [[modules/orkestrator-performance]] |
| **Admission control** | taak start pas als middelen (VRAM, RAM, cores) passen | [[modules/orkestrator-performance]] |
| **Preemption** | P0 komt binnen → lagere taken pauzeren op veilig punt | [[modules/orkestrator-performance]] |
| **System 1 / System 2** | snel pad (één agent) / volledige cyclus + team | [[modules/cognitie]] |
| **Metacognitie** | beslist snel/diep, model, team, budget, stoppen, vragen | [[modules/cognitie]] |
| **Judge-gate** | antwoord wordt pas getoond als de Judge het vertrouwt | [[modules/redeneren-debat]] |
| **Debatteam** | Planner, Solvers, Fact-checker, Critic, Specialist, Judge, Synthesizer | [[modules/redeneren-debat]] |
| **Conflict-team** | lost botsende feiten op en corrigeert geheugen | [[modules/redeneren-debat]] |
| **Research-team** | 11 rollen van vraag → geverifieerde kennis + data | [[modules/research-team]] |
| **Crawler-team** | Scout, Crawler, Reader, Ranker, Curator | [[modules/research-kennis]] |
| **Wereldmonitor** | nieuws/events-feeds → temporal KG + voorspellingen | [[modules/research-kennis]] |
| **Herkomstlabel (provenance)** | waar data vandaan komt; hook voor latere security-policy | [[01-REGELS]] |
| **Temporal KG** | knowledge graph met geldig-van/tot | [[modules/geheugen]] |
| **MMR** | Maximal Marginal Relevance: divers ophalen | [[modules/geheugen]] |
| **Tegenbewijs-slot** | gereserveerde plek in context voor bewijs dat ertegen ingaat | [[modules/context]] |
| **Dreaming** | geheugenonderhoud in idle (consolideren, samenvatten, decay) | [[modules/geheugen]] |
| **Tiering** | warm (SSD) ↔ koud (HDD) verplaatsen op basis van decay | [[04-HARDWARE-EN-OPSLAG]] |
| **Neurale layers** | eigen kleine netten (ONNX) die beslissingen leren | [[modules/neurale-layers]] |
| **Vliegwiel** | traces → SFT/DPO → QLoRA → beter model → betere traces | [[modules/training]] |
| **Poortwachter** | eval + schaduwrun beslist of nieuwe versie promoveert | [[modules/training]] |
| **Schaduwmodus** | nieuwe versie draait mee zonder effect, wordt vergeleken | [[modules/neurale-layers]] |
| **Thuismodel** | gpt-oss-20b, standaardmodel op de 5060 Ti | [[05-MODELLEN]] |
| **1060-team** | embedding + reranker + router, altijd geladen | [[05-MODELLEN]] |
| **Gate** | eindcriterium van een wave; groen = álles af | [[01-REGELS]] |
| **CARD** | `CARD.md` per crate: kaart van API, events, ingangen | [[12-NAVIGATIE]] |
| **Item-ID** | `W05.07` = wave 5, item 7; ook als code-tag `VX:W05.07` | [[12-NAVIGATIE]] |
