---
type: module
crate: vortex-tools
waves: [W08, W17, W18]
updated: 2026-10-31
---
# Tools en skills

## Toolsysteem
- Generiek framework: registratie, schema's, validatie.
- **Contract per tool**: invoer/uitvoer-schema, bijwerkingen (lezen / lokaal / extern), timeout, annuleerbaar, herhaalbaar, resources, netwerkbestemmingen.
- **Getypeerde tussenrepresentatie** per actie (actie, doel, invoer, verwachte uitvoer, afhankelijkheden, bijwerkingen), door code gevalideerd.
- **Pre- en postconditions.** Toolsucces ≠ doel bereikt.
- **Idempotent** via taak-ID; **transactioneel** (tijdelijke werkruimte → valideren → toepassen); diff/preview; herstelbaar via checkpoints.
- Native function calling + fallback (parser per model).
- Parallel + sequentieel, chaining/composition, effectanalyse bepaalt parallel.
- **Tool discovery**: alleen naam+beschrijving in context.
- **Programmatic tool calling** (W17): model schrijft script dat tools aanroept.
- Resultaten filteren met code; tool result caching; single-flight; negative caching.
- Herstel bij toolfouten: foutmelding verwerken, invoer aanpassen, andere route.
- Per-run capability handles (bv. één projectmap) — als techniek; toegangspolicy → ethics-laag.
- Pluginprotocol-versies; subprocess-supervisie.

## Tools per wave
- **W08**: bestanden (lezen/schrijven/bewerken/zoeken), web search (SearXNG) + pagina lezen, code interpreter (Python, shell) in sandbox, geheugen-tools, MCP-client, database-tool (SQLite), API-calling framework (generiek).
- **W16**: documentverwerking (PDF, DOCX, spreadsheets), audio, beeld genereren/bewerken.
- **W17**: browser automation, git/repository-tools, computer use, repo-niveau codebegrip.
- **W18**: Vortex schrijft en registreert eigen tools.

## Skills
- Eigen Vortex-skillformaat (superset).
- Converters heen en terug: **Claude Skills** (map met `SKILL.md`, frontmatter name/description, scripts/resources), **Cursor rules** (`.cursor/rules/*.mdc`: description, globs, alwaysApply), AGENTS.md.
- Progressive disclosure: naam+beschrijving in context, inhoud bij gebruik.
- Selectie via embeddings + skill selector; Cursor-globs als trigger.
- Scripts uit skills in sandbox.
- Vortex schrijft zelf skills uit geslaagde traces (procedural memory), succesrate + versie, slechte bijstellen/uitzetten.
- Hot-reload uit `config/skills/`.
