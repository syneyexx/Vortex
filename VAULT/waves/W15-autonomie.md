---
wave: W15
status: af  # alles gebouwd + getest; weekrun-gate 🔒 op commando; Tailscale + echte GPU-vrijgave op de PC
modules: [autonomie]
---
# W15 — Autonomie
Zie [[modules/autonomie]].

- [x] `W15.01` Idle-detectie + GPU direct vrijgeven bij gebruik — idle uit de sampler (OS-invoer + nvidia-smi → Dirigent, W04); P2/P3 wijken al; nieuw: GPU-bewaker voor beheerde llama-servers (`managed = true`): ontladen zodra een ander programma de GPU gebruikt en Vortex niets doet, terugladen na `restore_after_secs`, een chatverzoek laadt meteen (getest met de llama-server-testdubbel)
- [x] `W15.02` Leeragenda zelf uitvoeren in idle — taak `agenda` (trigger elke 4 u, alleen idle): gaten/betwiste feiten → research (quick), zwakke taaktypen → eval-taak; status open → researching → done
- [x] `W15.03` Taakqueue: prioriteit, afhankelijkheden, long-running projecten, multi-session continuity — `tasks` (P1–P3, afhankelijkheden, niet-eerder-dan, herhaalpogingen met backoff, pauzeren/hervatten/annuleren, herstel na herstart), executor (max 2 tegelijk, elke taak een run via de Dirigent), projecten met voortgang en doorlopende notities
- [x] `W15.04` Planning en triggers (tijd, gebeurtenis) — cron (lokale tijd, gemist moment één keer inhalen), interval, event (soort-patroon + `veld=waarde`-filter, event in de taakparams); standaardtriggers aan/uit in de GUI
- [x] `W15.05` Proactieve monitoring en suggesties — elke 30 min: open conflicten, mislukte taken, backup oud/mislukt, volle leeragenda, nieuws dat raakt aan gebruikers-/projectentiteiten, verlopen voorspellingen, schijfruimte → suggestie (één per dag), accepteren = taak
- [x] `W15.06` Dagelijkse briefing — 07:30 (inhalen als de PC uit stond): wereld, wat Vortex leerde, conflicten, taken/projecten, voorspellingen, suggesties, backup; model schrijft, vaste opmaak als terugval; ook als herinnering + melding
- [x] `W15.07` Lokale OpenAI-compatibele API (Cursor/eigen projecten) — `/v1/models`, `/v1/chat/completions` (stream + niet-stream): `vortex[-quick|-deep|-research]` = volledige pijplijn in een eigen sessie per gesprek; andere modellen = doorgeven aan de llama-server
- [~] `W15.08` Telefoontoegang via Tailscale + mobiele weergave — extra listener op het tailnet-adres (`[remote] tailscale = true`, `tailscale ip -4`), geen open poorten; **mobiele weergave vervalt (besluit gebruiker 2026-11-02)**; Tailscale zelf testen op de PC
- [x] `W15.09` Voortgangsdashboard "hoe slim is Vortex" — eval per maand per suite (+ trend), geheugengroei, layers, adapters, leeragenda, taken, kalibratie wereldmodel; achteruitgang > 5 pt → leeragenda (`weak_task`) → vliegwiel; maandelijkse eval-trigger
- [x] `W15.10` Automatische backups + restore-test — 03:15: online SQLite-backup + config naar de koude schijf, bewaren 7/4/12, restore-test (integrity_check, schemaversie, tellingen, proef-terugzetten); `vortex restore <bestand> [--with-config]`

## Gate
🔒 (weekrun op commando) Vortex draait een week zelfstandig (leren, briefings, backups) zonder handwerk; GPU wijkt altijd voor de gebruiker.
