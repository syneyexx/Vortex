---
type: module
dir: app/
waves: [W03]  # + elke wave voor eigen schermen
updated: 2026-10-31
---
# GUI

1-op-1 volgens `design/reference-chat.png`. Alle cijfers in de mockup zijn mockdata; app toont live data.

## Design tokens (eerst, één bestand)
Bijna-zwart/antraciet achtergrond · één oranje accent (glow actieve tab, voortgang, stopknop) · groen = voltooid · chroom vortex met oranje vonken · strak sans-serif · sectiekoppen met ruime letterafstand · afgeronde kaarten, dunne subtiele randen.

## Chatpagina
- **Topnav**: logo VORTEX · Chat · Research · Memory · Models · Studio · rechts Local workspace + System prompt. **Subtabs**: Dashboard · Chat · Agents.
- **Links**: vortex als **geanimeerde WebGL-shader** ("Intelligence in motion", Agents active / Knowledge sync) · Agent collaboration (rollen, verbindingslijnen, status) · Verdict-balk · Retrieved memory (type, titel, score + herkomst, bron-chip, mini knowledge graph).
- **Midden**: titel + subtitel · tabs Conversation / Sources (n) / Artifacts (n) · geschiedenis · New chat · gebruikersbericht · Vortex-antwoord met tool-kaarten (type, detail, duur, vinkje) · genummerde stappen · streaming-cursor + tok/s · achtergrondtaak-balk (pauze).
- **Invoer**: + bijlage · Quick / Deep / Research (Adaptive standaard) · modelkiezer · strategie · ronde oranje stopknop.
- **Statusbalk**: gezondheid per subsysteem · beide GPU's met VRAM-balk · actief model · tok/s sparkline.

## Overige pagina's
Instellingen (system prompt + rol-prompts live, versies + terugzetten, paden, modellen, gedrag, cloud-escalatie aan/uit) · Memory (bekijken, bewerken, vergeten, conflictregister, werkruimtes) · Models (registry, laden, VRAM, metingen) · Context Inspector · Lopend werk (pauzeren/stoppen alles) · Research · Studio · Dashboard "hoe slim is Vortex" · Dagelijkse briefing · Agents · Training/evals.

## Techniek
Events met volgnummer, snapshot + delta's, hervatten na reconnect · verbinding weg ≠ taak gestopt · getypeerde frames (tekst, toolstart, toolresultaat, artefact, waarschuwing, einde) · UI-event coalescing · artefacten als referentie, gedeeltelijk laden · incrementele UTF-8 · rich media · notificaties voor achtergrondtaken · voice (W15) · mobiel via Tailscale (W14) · visuele UI-controle: screenshot vs referentie (W16).
