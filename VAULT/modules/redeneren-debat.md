---
type: module
crate: vortex-agents
waves: [W09]
updated: 2026-10-31
---
# Redeneren en debatteam

Chat schakelt per bericht vanzelf tussen praten, code, research, analyse, rekenen. Achter moeilijke antwoorden zit een team. **Antwoord pas tonen als de Judge het vertrouwt.**

## Rollen (dynamisch samengesteld door metacognitie)
| Rol | Taak |
| --- | --- |
| Planner | opsplitsen, plan-state in workspace, goal decomposition |
| Solvers (2–3) | onafhankelijk uitwerken, andere hoek/temperatuur/evt. ander model |
| Fact-checker | claims extraheren → per claim bewijs: gestaafd / tegengesproken / onbevestigd, met bron |
| Critic / devil's advocate | zwakke plekken, tegenvoorbeelden |
| Specialist | bv. code-reviewer die tests echt draait |
| Judge | weegt bewijs, beslist of stuurt terug |
| Synthesizer | eindantwoord in gebruikersstijl |

## Regels
- Eenvoudig → alleen Synthesizer; moeilijk → volledig team.
- Objectieve checks (tests, berekening, bronnen) > stemmen; overeenstemming bewijst niets.
- Max 2–3 rondes, budget; geen vertrouwen → eerlijk onzeker / vraag stellen.
- **Offline-first**: lokale kennis altijd; online → automatische webcheck (vooral tijdgevoelig); offline → stil overslaan, label "lokaal geverifieerd".
- Agents sequentieel op 5060 Ti; claims splitsen op 1060; Judge/Fact-checker evt. ander model voor diversiteit.
- Verifier-guided candidate selection; reflexion/self-critique.
- Bewijsoverzicht per conclusie (bron, datum, bewijs, open onzekerheid).

## Conflict-team (direct bij elke botsing)
2 Onderzoekers (elk één kant) → Bronbeoordelaar (primair/secundair, actualiteit, onafhankelijkheid; kopieën = één bron) → Judge → Memory Writer (juiste versie + bron + datum, oude naar historie, afgeleiden verouderd).
- Raakt huidige vraag → eerst oplossen; anders direct op achtergrond.
- Eén webpagina wint niet automatisch; tijdgebonden → geldigheidsperiode; persoonlijke feiten: gebruiker wint; onbeslisbaar → "betwist" + melden.
- Conflictregister zichtbaar in GUI; alles gelogd en terug te draaien.

## Weergave
Codeblokken (highlighting, kopiëren, uitvoeren in sandbox, diff), tabellen, LaTeX, Mermaid, artefacten; statusregels in Agent collaboration-paneel; voortgang gescheiden van antwoord.
