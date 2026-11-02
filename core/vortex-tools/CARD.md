# CARD — `vortex-tools`

**Doel:** Toolregistry, contracten, idempotente/transactionele uitvoering, sandbox, ingebouwde tools, MCP-client, skills.
**VAULT-module:** VAULT/modules/tools-skills.md · **Waves:** W08 (af), W16, W17, W18

## Publieke API
| Item | Doet |
| --- | --- |
| `ToolHub::new(cfg, log, supervisor, secrets)` | alles samen; `core_set`, `specs`, `prompt_block`, `unwrap_call`, `connect_mcp` |
| `ToolHubConfig::from_snapshot` | uit `config/tools.toml` (searxng_url, python, workdir, skills_dir, limits, mcp.servers, apis, core) |
| `Tool` / `ToolContract` / `ToolCtx` / `ToolOutput` | eigen tool schrijven: contract + `call` |
| `ToolRegistry` | `register`, `unregister_origin`, `briefs`, `contract`, `closest` |
| `Executor::{call, execute, execute_plan}` | valideren → caps → pre → ledger → cache → uitvoeren (timeout/annuleren/retry) → transactie → post → events |
| `Action`, `Condition`, `plan::stages` | getypeerde acties, pre/postcondities, effectanalyse + chaining |
| `ledger::Ledger` | idempotentie per actie-ID (tabel `tool_ledger`) |
| `txn::FileTxn`, `diff::unified` | staging, preview, atomair toepassen |
| `Sandbox` | python/shell/script met limieten |
| `mcp::{McpClient, McpManager, McpServerCfg}` | MCP stdio + HTTP |
| `skills::{SkillStore, Skill}`, `skills::convert::*`, `skills::propose::*` | skills, converters, voorstellen |
| `CapabilityHandle` | technisch bereik per run |

## Events (schrijft)
`tool.call.{started,completed,failed,invalid}` (met `run_id`, preview), `tool.skill.{loaded,proposed}`, `tool.skills.reloaded`, `tool.mcp.connected`.

## Invarianten
- Zelfde actie-ID ⇒ hooguit één effect (ledger). Externe actie die onderbroken werd wordt nooit blind herhaald.
- Lokale schrijfacties zijn transactioneel: postconditie faalt ⇒ niets op schijf.
- Model ziet tool-uitvoer gefilterd/ingekort (max 12k tekens).

## Ingangen
`src/hub.rs` · `src/executor.rs` · `src/builtin/` · `src/mcp.rs` · `src/skills/` · schema: `vortex-core/migrations/0006_tools.sql`

## Tests
`cargo test -p vortex-tools` — 17 unit + 11 integratie (`tests/tools.rs`: idempotentie-gate, onderbroken extern, transactie/preview, herstelhints + caps, plan parallel/chaining, sandbox (limieten, annuleren), SQLite, MCP stdio + HTTP/SSE, skills import/selectie/hot-reload/voorstellen, cache/discovery).

## W17
`Tape`/`TapeMode` (record/replay via `ExecOptions.tape`), tools `tool_program` (programmatic tool calling), `git`, `repo_map`, `repo_search`.
