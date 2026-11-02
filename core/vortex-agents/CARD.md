# CARD — `vortex-agents`

**Doel:** Debatteam (W09), conflict-team (W09), research-team (W10).
**VAULT-module:** VAULT/modules/redeneren-debat.md · **Waves:** W09 (af), W10 (af)

## Publieke API
| Item | Doet |
| --- | --- |
| `team::compose(route, strategy, mode) -> Option<TeamPlan>` | dynamische teamsamenstelling |
| `debate::run(&DebateDeps, question, &TeamPlan) -> DebateOutcome` | Planner → Solvers ∥ → Critic/Fact-checker/Specialist → verifier-selectie → Judge-gate (rondes) |
| `debate::synthesizer_guidance(&Roles, &DebateOutcome)` | instructie voor het gestreamde eindantwoord |
| `factcheck::{extract_claims, check}` | claims + bewijs (geheugen, web als online) + labels |
| `specialist::{code, math}` | objectieve checks in de sandbox |
| `conflict::{resolve, rule_based}` | conflict-team → `MemoryStore::apply_resolution` |
| `roles::{Roles, ensure_defaults}` | rollen uit `config/roles/` |
| `connectivity::Connectivity` | online/offline (60 s cache, `force` voor tests/offline-modus) |
| `Reporter` | statusevents per rol |
| `research::{run, ResearchDeps, ResearchMode, choose_mode}` | research-team (W10): plan → vind → lees/extraheer → analyseer → verifieer → oordeel → rapport → leer |
| `research::corpus_synthesis` | vraag over een hele verzameling documenten |
| `records::{validate, schema, render, KINDS}` | extractor-schema's |
| `world::{integrate, hypothesize, hypotheses, add_prediction, predict, predictions, resolve_due, calibration, summarize, events, event_detail}` | wereldmodel (W14): KG-integratie, hypotheses, voorspellingen + toetsen + kalibratie, samenvattingen |
| `world::{add_player, players, player, study_player, predict_player}` | spelers per sector (alleen publiek), "wat zou X doen?" |

## Events (schrijft)
`agent.team.composed`, `agent.role.{started,completed}`, `agent.judge.verdict`, `agent.evidence.compiled`, `research.{run.started,subquestion.updated,source.found,record.extracted,report.written}`, `world.prediction.{made,resolved}`; via geheugen: `memory.conflict.{resolved,rolled_back}`.

## Invarianten
- Objectieve checks wegen zwaarder dan overeenstemming; antwoord pas na het Judge-oordeel.
- Offline: nooit webcalls; scope "lokaal".
- Elke conflictbeslissing is terug te draaien (`conflict_history`).

## Tests
`cargo test -p vortex-agents` — unit (team, rollen, json, specialist) + `tests/team.rs` (rekencheck kiest juiste kandidaat, Judge-revise/onzeker, offline lokaal bewijs, conflict-team: regel/periodes/rollback/betwist) + `tests/research.rs` (standaard end-to-end met nep-SearXNG en twee domeinen, offline lokaal, corpus) + `tests/world.rs` (KG-integratie, hypotheses, voorspellen + toetsen + kalibratie, samenvattingen, spelers).

## W18
`agentmem::{record, recall, private_ws, shared_ws}`.
