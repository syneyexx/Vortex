# CARD — `vortex-eval`

**Doel:** meten boven geloven: eval-sets, automatische scoring, resultaten per versie, vergelijken, retrieval-eval, lekkagecontrole, performance-harnas.
**VAULT-module:** VAULT/modules/observability-eval.md · **Waves:** W06, W17

## Publieke API
| Item | Doet |
| --- | --- |
| `load_dir` / `load_suite` | `evals/sets/*.jsonl` (één taak per regel, `//` = commentaar) |
| `Check` | contains, contains_any, not_contains, regex, number, python_tests, json_fields, cites_source, admits_unknown, conflicts_at_least |
| `Subject` (trait) | het geteste systeem (server: `VortexSubject`, via de chat-pipeline zonder opslag, geïsoleerde `__eval_*`-werkruimtes) |
| `Runner::run_suite` | draait, scoort, schrijft `eval_runs`/`eval_results` + events; `requires: "Wnn"` → eerlijk "skip" |
| `compare(run)` | t.o.v. vorige run van dezelfde suite: delta, regressies, verbeteringen, verdict (ROOD bij > 2 punt slechter) |
| `retrieval::run` | recall@1/3/5, MRR, irrelevant@5, geen-antwoord — op geïsoleerd geheugen |
| `leakage::check` | 5-gram-containment (≥ 0,6) eval-tekst ↔ corpus |
| `perf::{measure, PerfTargets}` | percentielmetingen (🔒 volledige suite op commando) |

## Data
`evals/sets/` (redeneren, code, geheugen, conflicten, eerlijkheid, tools*, research*) · `evals/retrieval.json` · schema `0004_eval.sql`.

## Tests
`cargo test -p vortex-eval` — checks, lekkage, percentielen, runner+vergelijking met nep-subject, validatie van de repo-sets, retrieval-eval op de repo-set.
