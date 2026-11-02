---
wave: W03
status: bezig  # code af 2026-11-02; gate (chat met gpt-oss-20b) op de PC
modules: [gui, context]
---
# W03 — GUI en chat
Zie [[modules/gui]], `design/reference-chat.png`.

## Server
- [x] `W03.01` `vortex-server`: eventstream met volgnummers, snapshot + delta's, resume na reconnect
- [x] `W03.02` Getypeerde frames; verbinding weg ≠ taak gestopt (runpolicy) — frames = getypeerde events uit het log (JSON over WebSocket)
- [~] `W03.03` UI-event coalescing; artefacten als referentie — coalescing af (server 40 ms/512 tekens, client per animatieframe); artefacten als referentie volgen met tools (W08)

## Frontend
- [x] `W03.04` Design tokens-bestand (kleuren, radii, spacing, typografie, schaduw/glow)
- [x] `W03.05` Topnav + subtabs + routing voor alle pagina's (lege staten waar functie later komt — nooit nep-data)
- [x] `W03.06` Vortex WebGL-shader (geanimeerd, performant, pauzeert als tab inactief)
- [x] `W03.07` Chat: streaming berichten, markdown, codeblokken (highlighting, kopiëren, diff), tabellen, LaTeX, Mermaid
- [~] `W03.08` Invoerbalk: bijlage, Quick/Deep/Research/Adaptive, modelkiezer, strategie, stopknop (echte cancellation) — alles af incl. echte stop; bijlagen = tekstbestanden ≤ 200 kB; modelkiezer stuurt het model mee, **echt wisselen via `ModelServer` op de PC koppelen**
- [x] `W03.09` Tool-kaarten en Agent collaboration-paneel aangesloten op events (vult zich vanaf W08/W09)
- [x] `W03.10` Achtergrondtaak-balk + Lopend werk-pagina (pauzeren/stoppen) — **af:** pauzeren/hervatten via Dirigent (`/api/runs/:id/pause|resume`), stoppen, Dirigent-kaart op Lopend werk
- [~] `W03.11` Statusbalk: gezondheid per subsysteem, beide GPU's live VRAM, actief model, tok/s sparkline — af; GPU-balken lezen nvidia-smi — **verifiëren op de PC**
- [~] `W03.12` Instellingen: system prompt + rol-prompts, live vanaf volgend bericht, "nu toepassen", versiegeschiedenis + terugzetten; paden; modellen; gedrag — system prompt + rol-prompts live, versies, terugzetten af; paden/modellen/gedrag nog via `config/*.toml` (formulieren later)
- [x] `W03.13` Sessies: opslaan/herstellen, branching/forking, nieuwe chat, geschiedenis
- [x] `W03.14` Notificaties voor achtergrondtaken
- [x] `W03.15` Visuele vergelijking met referentie (screenshot naast elkaar) — Playwright-screenshot naast referentie; gevonden en opgelost: herladen tijdens streamen verloor tekst, kolom-overflow, shader-naad. **Herhalen met echte GPU-data op de PC**

## Gate
Chatten met gpt-oss-20b in exact de referentiestijl; herladen tijdens streamen verliest niets; stop stopt echt; alles in event log.
