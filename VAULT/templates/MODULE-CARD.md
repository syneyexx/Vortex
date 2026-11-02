---
type: template
---
# CARD — `<crate-of-map>`
<!-- Kopieer naar <crate>/CARD.md. Max ~80 regels. Bijwerken bij elke wijziging van de publieke API. -->

**Doel (1 zin):**
**VAULT-module:** VAULT/modules/<naam>.md · **Waves:** W..

## Publieke API (wat anderen mogen gebruiken)
| Item | Soort | Doet |
| --- | --- | --- |
| `Foo::bar()` | fn | … |

## Afhankelijk van
- crates: …
- processen/externe diensten: …

## Events
- **schrijft:** `domein.onderwerp.gedaan`, …
- **leest/reageert op:** …

## Invarianten (moet altijd waar zijn)
- …

## Ingangen (begin hier met lezen)
| Bestand | Waarom |
| --- | --- |
| `src/lib.rs` | publieke API |

## Tests
- `cargo test -p <crate>` · belangrijkste: …

## One-brain-check
- [ ] event log · [ ] workspace · [ ] geheugen · [ ] metacog/neural · [ ] GUI
