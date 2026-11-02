---
wave: W08
status: af  # gate: idempotentie + skill-import groen in tests; eval toolgebruik met echt model op de PC
modules: [tools-skills]
---
# W08 — Tools en skills
Zie [[modules/tools-skills]].

- [x] `W08.01` Toolregistry + contracten (schema, bijwerkingen, timeout, annuleerbaar, herhaalbaar, resources) — `ToolRegistry` + `ToolContract` (schema in/uit, effect lezen/lokaal/extern, timeout, annuleerbaar, idempotent, resources, netwerkbestemmingen, herkomst, versie)
- [x] `W08.02` Getypeerde actie-representatie + validatie — `Action` (doel, invoer, verwacht, afhankelijkheden, pre/post) + JSON-schema-validatie met milde coercie
- [x] `W08.03` Pre/postconditions-framework — `Condition` (pad bestaat/afwezig, bestand bevat, uitvoer bevat, veld); postcondities zien de klaargezette transactie
- [x] `W08.04` Idempotentie (taak-ID), transactionele uitvoering, diff/preview, checkpoints — `tool_ledger` (migratie 0006): retry = replay, zelfde ID + andere invoer = geweigerd, onderbroken externe actie nooit blind herhaald; `FileTxn` (staging → diff → atomair toepassen, rollback); dry-run/preview; checkpoint per toolronde
- [x] `W08.05` Effectanalyse → parallel/sequentieel, chaining — `plan::stages` (lezers parallel, schrijvers op zelfde doel/resource na elkaar, barrières) + chaining `$ref`/`${a.text}`
- [x] `W08.06` Sandbox-procesgroep (Python, shell) met resource-limieten — `Sandbox` via de Supervisor: eigen procesgroep, tijd/CPU/geheugen (Unix ulimit, Windows Job Object)/uitvoer-limieten, schone omgeving, annuleren stopt de boom
- [x] `W08.07` Tools: bestanden (lezen/schrijven/bewerken/zoeken), lokale mappen doorzoeken, web search (SearXNG), pagina lezen, code interpreter, SQLite-tool, generiek API-calling framework, geheugen-tools — read/write/edit/delete_file, list_dir, search_files, find_files, web_search (SearXNG), web_fetch (HTML→tekst), python, shell, sqlite_query/exec, http_get/http_request + API's uit `config/tools.toml`, memory_search/write/forget (server)
- [x] `W08.08` MCP-client (stdio + HTTP), MCP-tools in registry — `McpClient` stdio + streamable HTTP (JSON/SSE, sessie-ID), paginering, effect uit annotaties, herverbinden; tools als `mcp__server__tool`
- [x] `W08.09` Tool discovery (naam+beschrijving, schema bij gebruik) — kernset met schema, rest naam+beschrijving in de prompt; `describe_tool` + `call_tool`
- [x] `W08.10` Resultaatfiltering met code, tool result caching — `ResultFilter` (pointer/grep/head/tail/max), automatisch inkorten (begin+eind), `ToolCache` (TTL) voor leestools
- [x] `W08.11` Herstel bij toolfouten — `ToolError` met soort + hint voor het model (schema, bijna-match bij edit, dichtstbijzijnde toolnaam); retry met backoff alleen bij idempotent
- [x] `W08.12` Capability handles per run (technisch) — `CapabilityHandle` (mappen, netwerk, schrijven, processen, toolglobs); standaard alles open (geen remmen tot W19)
- [x] `W08.13` Skills: eigen formaat, converters Claude Skills ⇄ Cursor rules ⇄ AGENTS.md, import/export, progressive disclosure, selectie (embeddings + globs), scripts in sandbox, hot-reload — eigen formaat (superset Claude), converters Claude ⇄ Cursor ⇄ AGENTS.md (heen en terug getest), progressive disclosure (`load_skill`), selectie (trefwoorden + embeddings + globs + alwaysApply), scripts in de sandbox (`run_skill_script`), hot-reload, statistiek + automatisch uitzetten
- [x] `W08.14` Skill-voorstellen uit geslaagde traces (basis) — `propose::observe` na geslaagde runs met ≥ 2 toolstappen; zelfde patroon telt op; accepteren → skill (GUI Instellingen)
- [x] `W08.15` Tool-kaarten in GUI live — tool-kaarten live per run (status, duur, doel; klik = invoer, diff, uitvoer/fout) + beheer in Instellingen → Tools & skills

## Gate
Retry van elke actie veroorzaakt nooit dubbel effect; geïmporteerde Claude- en Cursor-skills werken; eval toolgebruik groen.
