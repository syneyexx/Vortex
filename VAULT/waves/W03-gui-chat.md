---
wave: W03
status: open
modules: [gui, context]
---
# W03 — GUI en chat
Zie [[modules/gui]], `design/reference-chat.png`.

## Server
- [ ] `W03.01` `vortex-server`: eventstream met volgnummers, snapshot + delta's, resume na reconnect
- [ ] `W03.02` Getypeerde frames; verbinding weg ≠ taak gestopt (runpolicy)
- [ ] `W03.03` UI-event coalescing; artefacten als referentie

## Frontend
- [ ] `W03.04` Design tokens-bestand (kleuren, radii, spacing, typografie, schaduw/glow)
- [ ] `W03.05` Topnav + subtabs + routing voor alle pagina's (lege staten waar functie later komt — nooit nep-data)
- [ ] `W03.06` Vortex WebGL-shader (geanimeerd, performant, pauzeert als tab inactief)
- [ ] `W03.07` Chat: streaming berichten, markdown, codeblokken (highlighting, kopiëren, diff), tabellen, LaTeX, Mermaid
- [ ] `W03.08` Invoerbalk: bijlage, Quick/Deep/Research/Adaptive, modelkiezer, strategie, stopknop (echte cancellation)
- [ ] `W03.09` Tool-kaarten en Agent collaboration-paneel aangesloten op events (vult zich vanaf W08/W09)
- [ ] `W03.10` Achtergrondtaak-balk + Lopend werk-pagina (pauzeren/stoppen)
- [ ] `W03.11` Statusbalk: gezondheid per subsysteem, beide GPU's live VRAM, actief model, tok/s sparkline
- [ ] `W03.12` Instellingen: system prompt + rol-prompts, live vanaf volgend bericht, "nu toepassen", versiegeschiedenis + terugzetten; paden; modellen; gedrag
- [ ] `W03.13` Sessies: opslaan/herstellen, branching/forking, nieuwe chat, geschiedenis
- [ ] `W03.14` Notificaties voor achtergrondtaken
- [ ] `W03.15` Visuele vergelijking met referentie (screenshot naast elkaar)

## Gate
Chatten met gpt-oss-20b in exact de referentiestijl; herladen tijdens streamen verliest niets; stop stopt echt; alles in event log.
