---
type: regels
updated: 2026-11-02
---
# Navigatie — werken in een codebase van 500K–1M regels zonder alles te lezen

Doel: Claude (en jij) vinden elk stuk code in **≤ 3 stappen**, zonder het project te lezen. De VAULT is de kaart, de code is het terrein.

## De 3-stappen-route
1. **Wat?** → [[FUNCTIES]] of [[TODO]] → item-ID (bv. `W04.07`).
2. **Waar?** → [[20-CODEMAP]] → crate/map + `CARD.md` van die crate.
3. **Precies waar?** → `python tools/vault/vault.py find W04.07` of `rg "VX:W04.07"` → bestand + regel.

Pas daarna bestanden openen, en alleen de bestanden die de CARD aanwijst.

## Navigatieregels (gelden altijd, naast [[01-REGELS]])
- **N1 — Lees nooit "het hele project".** Start bij de CARD, niet bij de code. Geen `cat` van mappen; zoeken met `rg`, lezen met regelbereik.
- **N2 — Elke crate/package heeft een `CARD.md`** (template: `templates/MODULE-CARD.md`), max ~80 regels: doel, publieke API, afhankelijkheden, events in/uit, invarianten, ingangen (bestanden), tests. Wie de publieke API verandert, werkt de CARD bij in dezelfde commit.
- **N3 — Code-tags.** Code die een wave-item implementeert krijgt één comment `// VX:W04.07` (Python: `# VX:W04.07`) bij de hoofd-ingang (niet overal). `vault.py check` valideert ze.
- **N4 — Grenzen via traits/interfaces.** Crates praten alleen via hun publieke API in `lib.rs`/`api.rs`. Nooit `pub` interne modules uit een andere crate gebruiken. Zo hoef je alleen de API te lezen, niet de binnenkant.
- **N5 — Bestandsgrootte.** Richtlijn ≤ 600 regels per bestand, ≤ 15 bestanden per map voordat je opsplitst. Grote bestanden zijn duur om te lezen.
- **N6 — Eén plek per begrip.** Gedeelde typen (IDs, events, fouten) staan in `vortex-types`. Event-namen staan in één register ([[13-CONVENTIES]]). Geen kopieën.
- **N7 — Afvinken = VAULT bijwerken.** Item af → `[x]` in de wave, `vault.py todo`, regel in [[LOG]]. `vault.py check` moet groen zijn vóór commit.
- **N8 — Context is schaars.** Lees eerst `LOG.md` onderaan (laatste 2 sessies), daarna alleen de actieve wave. Een notitie > 150 regels wordt opgesplitst.
- **N9 — Onzeker waar iets hoort?** Eerst [[20-CODEMAP]] en [[14-WOORDENLIJST]]; staat het er niet in, dan is het een nieuwe beslissing → [[03-BESLUITEN]].

## Tools
| Commando | Doet |
| --- | --- |
| `python tools/vault/vault.py status` | voortgang per wave |
| `python tools/vault/vault.py todo` | [[TODO]] opnieuw genereren |
| `python tools/vault/vault.py check` | dode links, dubbele/ontbrekende ID's, code-tags, TODO actueel |
| `python tools/vault/vault.py find <term of ID>` | zoekt in VAULT + code-tags |
| `rg "VX:W07"` | alle code van wave 7 |

## Waarom zo
Op 1M regels past ~0,5% van de code in één context. Zonder kaart gaat elke sessie op aan zoeken en vergeet je afspraken. Met kaart + CARD's + ID's leest Claude per taak typisch 3–10 bestanden.
