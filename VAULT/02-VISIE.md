---
type: visie
updated: 2026-10-31
---
# Visie

**Vortex** is een persoonlijk, lokaal AI-framework (harness). Naam-symboliek: informatie gaat erin, wordt in de kern verwerkt, en komt verfijnd eruit. Alleen voor persoonlijk gebruik.

## Doel
Framework + taalmodel vormen samen **één brein**. Het framework is "50% van het brein": het redeneert mee, onthoudt, leert en traint eigen netwerken, zodat de output slimmer wordt dan het kale model en niet beperkt blijft tot de modelgewichten.

Vortex moet kunnen: chatten, programmeren, high reasoning, research (web + bestanden + ebooks), leren uit datasets (Hugging Face), modelgewichten trainen (LoRA/QLoRA), de wereld begrijpen via nieuws/events, leren van grote spelers per sector, skills importeren en gebruiken, en later traden.

## Principes
1. **Eén brein, twee helften** — eigen cognitie + eigen neurale layers in het framework, naast het LM.
2. **Twee leerlussen** — framework leert (geheugen, skills, kleine netten); model leert (LoRA). Beide gevoed door één event log.
3. **Offline-first** — lokale kennis altijd eerst; internet is automatische extra check als er verbinding is, anders stil overslaan.
4. **Meten boven geloven** — eigen eval-suite beslist; slechter = automatisch terug.
5. **Antwoord pas na vertrouwen** — Judge beslist; lukt dat niet binnen budget, dan eerlijk zeggen wat onzeker is of iets vragen.
6. **Eén taak tegelijk, modellen wisselen per taak** — scheduler bewaakt VRAM, actieve chat heeft voorrang.
7. **Alles verwisselbaar** — modellen, opslag, tools achter interfaces.
8. **Betrouwbaar boven indrukwekkend** — na crash/afsluiten weet Vortex precies wat af is, wat liep, wat veilig hervat kan.

## Eerlijke grens
Een sterk framework maakt een lokaal model duidelijk slimmer, maar het plafond van het model blijft bestaan. Het vliegwiel (distillatie + LoRA) tilt dat plafond langzaam op; de eval-suite laat zien hoeveel.

Zie ook: [[11-ONE-BRAIN]], [[03-BESLUITEN]], [[waves/WAVES-OVERZICHT]]
