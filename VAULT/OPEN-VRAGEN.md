---
type: brainstorm
updated: 2026-11-02
---
# Open vragen en brainstorm

Punten om samen te beslissen (vanavond 19:00 of later). Besluit genomen → regel in [[03-BESLUITEN]] en hier doorstrepen.

## Vanavond eerst (W00, op de PC)
- [ ] Hardware-checks uit [[waves/W00-fundament]]: SSD-type + vrije ruimte, `nvidia-smi` met beide GPU's, driverversie.
- [ ] Repo klonen op de PC: waar? (voorstel: SSD, bv. `C:\dev\Vortex`, níet in OneDrive/Desktop-sync).
- [ ] Windows native of (deels) WSL2? Raakt CUDA, llama.cpp-builds, Python-training (Unsloth werkt het best op Linux/WSL). Voorstel: kern + GUI native Windows, training eventueel WSL2 → meten.
- [ ] Versies te verifiëren vóór installeren (niet blind overnemen uit de notities): CUDA 12.8/12.9 voor sm_61 + sm_120, PyTorch-wheel voor Pascal, llama.cpp-build met `--moe-cache-mib` en NVFP4.

## Architectuur — voorstellen van Claude (2026-11-02)
- [ ] **`vortex-types`-crate** toevoegen voor gedeelde ID's/events/fouten → voorkomt afhankelijkheidscycli tussen crates. Zie [[20-CODEMAP]].
- [ ] **`vortex-crawler`** als eigen crate i.p.v. in `vortex-agents` (crawler is I/O-zwaar en heeft eigen ritme).
- [ ] **`proto/`-map** voor gRPC-contracten (kern ⇄ Python-worker ⇄ GUI) als één bron.
- [ ] **W01 is groot (24 items).** Opsplitsen in W01a (event log, state machine, cancellation, checkpoints) en W01b (Dirigent, CPU-affiniteit, Job Objects, HDD-wachtrij)? De Dirigent is pas echt nodig als er meerdere soorten werk zijn.
- [ ] **Verticale plak eerst?** Een minimale end-to-end (chat → model → antwoord, met event log) in W01–W03 geeft vroeg iets werkends; de gates blijven gelden per wave.

## Spanningen in het ontwerp (bewust kiezen)
- [ ] **Judge-gate vs. "eerste token < 1 s".** Als het antwoord pas na Judge-oordeel verschijnt, kan TTFT bij System 2 niet < 1 s. Voorstel: doel < 1 s geldt voor Quick/System 1; bij Deep tonen we direct de voortgang (agent-paneel) en het antwoord na de Judge.
- [ ] **Eén taak tegelijk op de 5060 Ti vs. debatteam (7 rollen sequentieel).** Deep-antwoorden kosten meerdere modelpassen. Meten in W08; eventueel Solvers in één batch (parallelle slots in `llama-server`).
- [ ] **Model-RAM-cache 12 GB** concurreert met Python-worker en DB-caches; RAM-budget pas vastzetten na metingen.
- [ ] **250 GB SSD** is krap met ~60–70 GB modellen + index + training-basismodel. Bewaken vanaf W00.

## Ideeën (nog niet ingepland)
- Eén `vortex` CLI naast de GUI (status, eval draaien, model laden) — handig voor debuggen en voor Claude.
- `just`/`make`-bestand met vaste commando's (build, test, check, eval).
- Eval vanaf W00 al een "rookproef": 10 vragen die elke wave moeten blijven werken.
