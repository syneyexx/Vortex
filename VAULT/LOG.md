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

**Actieve wave:** W00 (nog niet gestart)
