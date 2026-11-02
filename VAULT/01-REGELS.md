---
type: regels
updated: 2026-10-31
---
# Werkregels (door gebruiker vastgesteld, 2026-10-31; regel 7 op 2026-11-02)

1. **One brain.** Het brein werkt als één geheel; alle systemen zijn verbonden. Geen module die los van de rest leeft. Zie [[11-ONE-BRAIN]].
2. **Hoog niveau, meest efficiënt.** Werk op topniveau, kies steeds de efficiëntste route.
3. **Zelfcontrole.** Kijk eigen werk na op fouten en gaten voordat iets "klaar" heet.
4. **Bij twijfel onderzoeken.** Niet gokken; eerst uitzoeken (docs, bronnen, testen).
5. **Gate groen = alles af.** Een groene gate telt niet als 3 van 10 functies werken. Elk item in de wave tot in detail af, getest en verbonden.
6. **Geen remmen tot het commando.** Bouw geen beperkingen, toestemmingsprompts, blokkades of policies tot de gebruiker zegt: **"WE GAAN DE ETHICS EN SECURITY LAYER MAKEN"**. Alles van die soort staat verzameld in [[GEPARKEERD]].

7. **Kleine tests altijd, grote tests op commando.** (2026-11-02) Kleine tests (unit-, integratie-, snelle regressietests) schrijven en draaien we tussendoor bij elk item: die vangen fouten vroeg. **Grote tests** draaien pas als de gebruiker het zegt (bv. **"GROTE TESTEN NU"**). Grote tests = 24-uurs soak-test, meerdaagse/weeklange stabiliteitsruns, volledige eval-suite-runs, performance-testsuites, fault-injection-campagnes, ablations, TB-schaal-simulaties. Ze worden wél voorbereid (scripts/harnas klaar), alleen niet gestart. Items en gates die ervan afhangen zijn gemarkeerd met 🔒; de rest van zo'n gate telt gewoon.

## Uitleg bij regel 6 (afgestemd met gebruiker)
- **Wel nu bouwen (techniek, geen rem):** procesisolatie zodat een crash de kern niet meeneemt; API-sleutels buiten de modelcontext (secrets broker); ongeldige toolcall = fout, nooit actie; idempotentie; checkpoints.
- **Hooks wel, policy niet:** herkomstlabels op data worden nu al vastgelegd (nodig voor bronverwijzing en one brain); de policies die erop handelen komen in de ethics & security layer.

## Vaste afspraken
- Trading pas bij **"TRADING NU MAKEN"**, als de rest optimaal werkt.
- Claude bouwt geen functies voor privégegevens van personen (adressen, familie) of profilering van privépersonen. Gebruiker heeft bevestigd dat niet nodig te hebben.
- Werk vindt plaats op de PC van de gebruiker (avondsessies vanaf 19:00). Repo: GitHub `syneyexx/Vortex`; brainstormen en VAULT-werk kan ook via Claude Code op het web.

## Navigatieregels
Voor een codebase van 500K–1M regels gelden ook N1–N9 uit [[12-NAVIGATIE]] (CARD per crate, code-tags `VX:Wnn.mm`, nooit het hele project lezen) en de conventies in [[13-CONVENTIES]].

## Definition of Done (per item in een wave)
- [ ] Code af, geen TODO's of stubs
- [ ] Verbonden met event log, tracing en waar relevant geheugen
- [ ] Kleine tests geschreven en groen (regel 7); grote tests alleen op commando
- [ ] Zichtbaar/bedienbaar in GUI waar dat hoort
- [ ] Zelf-review gedaan op fouten en gaten
- [ ] `CARD.md` van de crate bijgewerkt; code-tag `VX:<item-ID>` bij de ingang
- [ ] [[LOG]] en indien nodig [[03-BESLUITEN]] bijgewerkt; `vault.py check` groen
