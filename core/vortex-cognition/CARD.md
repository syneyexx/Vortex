# CARD — `vortex-cognition`

**Doel:** Cognitieve cyclus, Global Workspace, doelstapel, metacognitie, planning, leeragenda, user model.
**VAULT-module:** VAULT/modules/cognitie.md · **Waves:** W07 (af), W12, W18

## Publieke API
| Item | Doet |
| --- | --- |
| `cycle::{Cycle, Phase}` | 8 fasen; `enter`/`finish` → event `cognition.phase.completed` |
| `workspace::Workspace` | Global Workspace per run: doelen (`push_goal`, `drop_goal`, `ready_goals`), hypotheses, notities, strategie, `steer`/`take_steering`; events |
| `router::{route, refine_with_lm, TaskType, Route}` | heuristische route; LM-label bij twijfel (`LM_LABEL_BELOW`) |
| `metacog::{decide, switch_on, Signals, Strategy, Path, PlanStyle, Prediction}` | snel/diep, plan, effort, budget, selective prediction; switching bij stall |
| `planning::{make_plan, execute_plan, critique, react, ToolRunner}` | plan-and-execute, reflexion, ReAct |
| `salience::tag` | urgent/nieuw/tegenstrijdig/valentie |
| `agenda::Agenda` | leeragenda (kennisgaten, betwiste feiten) in SQLite |
| `usermodel::UserProfile` | profiel uit geheugen + feedback; `prompt_block()` |
| `llm::{CogLlm, GenOpts}` | wat cognitie van een taalmodel vraagt (server koppelt aan backend) |

## Afhankelijk van
vortex-core, vortex-types, vortex-models (berichttypen), vortex-memory (user model). Richting: `tools/check_deps.py`.
