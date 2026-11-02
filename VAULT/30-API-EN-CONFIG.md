---
type: referentie
---
# API, CLI en config (gegenereerd)

Gegenereerd door `python tools/vault/api_overview.py` — niet met de hand bewerken.
Kern: `http://127.0.0.1:7420`; events live via WebSocket `/api/events?from_seq=N`.

## CLI (`core/target/release/vortex[.exe]`, globale opties `--config config`, `--data <map>`)
| Commando | Doet |
| --- | --- |
| `vortex serve` | Start de kern + API voor de GUI. |
| `vortex status` | Toon status van GPU's, modelservers en kern. |
| `vortex eval` | Eval draaien via de draaiende kern en vergelijken met de vorige run (W06). |
| `vortex perf` | 🔒 Prestatiedoelen meten (alleen op commando; --quick = korte harnas-check). |
| `vortex soak` | 🔒 Soak-test (24 u alleen op commando): gesimuleerd gebruik + RSS-trend. |
| `vortex tierbench` | 🔒 Tiering-benchmark (TB-simulatie alleen op commando): N synthetische herinneringen naar de koude laag (zstd), dan p50/p95 zoeklatency over de koude blokken. |
| `vortex faults` | 🔒 Fault injection (W17.06, GROTE TEST — alleen op commando): Vortex in een tijdelijke datamap met de echte config, modelverkeer via een fout-proxy (weg, afgekapt, verminkt, traag), crash + herstart, volle schijf bij backup. Rapport + exitcode. |
| `vortex ablate` | 🔒 Ablations (W17.02, GROTE TEST — alleen op commando): elke component uit vs alles aan op de eval-suites, gepaard; toont per component de meetbare waarde. |
| `vortex specbench` | Speculative decoding meten (W17.08, op de PC met GPU): tok/s met en zonder draft-model. Zet daarna `spec_gain = <uitkomst>` bij het model in models.toml (≥ 1,05 → aan). |
| `vortex restore` | Backup terugzetten (W15.10). De kern moet gestopt zijn; de huidige database wordt eerst bewaard als `vortex.db.voor-restore`. |
| `vortex runs` | Toon runs. |

## Configbestanden (`config/`, hot-reload)
| Bestand | Inhoud |
| --- | --- |
| `autonomy.toml` | Autonomie (W15). Zie VAULT/modules/autonomie.md en core/vortex-server/src/autonomy/. |
| `cloud.toml` | Optionele cloud-escalatie (W17.17). STANDAARD UIT: Vortex blijft volledig lokaal. |
| `frontier.toml` | Frontier (W18). Alles STANDAARD UIT — pas aanzetten als het experiment meetbare winst laat |
| `media.toml` | Multimodaal (W16). Zie core/vortex-server/src/media.rs en worker/src/vortex_worker/media.py. |
| `models.toml` | Model-registry (W00 basis; volledige velden + validatie in W02). Hot-reload. |
| `paths.toml` | Opslagpaden. SSD = warm, HDD = koud. Aanpassen aan je schijfletters (zie VAULT/04-HARDWARE-EN-OPSLAG.md). |
| `tools.toml` | Tools en skills (W08). Zie VAULT/modules/tools-skills.md en core/vortex-tools/CARD.md. |
| `training.toml` | Training en vliegwiel (W13). Zie VAULT/modules/training.md en core/vortex-server/src/training_api.rs. |
| `world.toml` | Wereldmonitor (W14). Zie VAULT/modules/research-kennis.md en core/vortex-server/src/world_api.rs. |

## REST-routes
| Pad | Methoden | Bron |
| --- | --- | --- |
| `/api/agenda` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/agenda/:id` | POST | `core/vortex-server/src/observe_api.rs` |
| `/api/analytics` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/autonomy` | GET | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/backups` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/briefings` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/gpu` | GET | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/progress` | GET | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/projects` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/projects/:id` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/suggestions` | GET | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/suggestions/:id/:action` | POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/suggestions/scan` | POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/tasks` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/tasks/:id` | GET | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/tasks/:id/:action` | POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/triggers` | GET, POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/autonomy/triggers/:id` | POST | `core/vortex-server/src/autonomy/api.rs` |
| `/api/causal/counterfactual` | POST | `core/vortex-server/src/frontier.rs` |
| `/api/causal/effects` | POST | `core/vortex-server/src/frontier.rs` |
| `/api/diagnostics/logprobs` | POST | `core/vortex-server/src/quality_api.rs` |
| `/api/eval/leakage` | GET | `core/vortex-server/src/eval_api.rs` |
| `/api/eval/retrieval` | POST | `core/vortex-server/src/eval_api.rs` |
| `/api/eval/run` | POST | `core/vortex-server/src/eval_api.rs` |
| `/api/eval/runs` | GET | `core/vortex-server/src/eval_api.rs` |
| `/api/eval/runs/:id` | GET | `core/vortex-server/src/eval_api.rs` |
| `/api/eval/suites` | GET | `core/vortex-server/src/eval_api.rs` |
| `/api/events` | GET | `core/vortex-server/src/api.rs` |
| `/api/experiments` | GET | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/:id` | GET | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/ab` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/ablation` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/assoc` | POST | `core/vortex-server/src/frontier.rs` |
| `/api/experiments/compression` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/metamorphic` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/positional` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/prompt_opt` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/experiments/shadow` | POST | `core/vortex-server/src/experiments.rs` |
| `/api/frontier` | GET | `core/vortex-server/src/frontier.rs` |
| `/api/health` | GET | `core/vortex-server/src/api.rs` |
| `/api/ingest` | POST | `core/vortex-server/src/research_api.rs` |
| `/api/ingest/documents` | GET | `core/vortex-server/src/research_api.rs` |
| `/api/media` | GET | `core/vortex-server/src/media.rs` |
| `/api/media/analyze` | POST | `core/vortex-server/src/media.rs` |
| `/api/media/file/:name` | GET | `core/vortex-server/src/media.rs` |
| `/api/media/imagine` | POST | `core/vortex-server/src/media.rs` |
| `/api/media/page` | POST | `core/vortex-server/src/media.rs` |
| `/api/media/speak` | POST | `core/vortex-server/src/media.rs` |
| `/api/media/transcribe` | POST | `core/vortex-server/src/media.rs` |
| `/api/memory` | GET, POST | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/:id` | GET, PATCH | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/:id/media` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/backup` | POST | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/belief-experiment` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/conflicts` | GET | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/conflicts/:id/resolve` | POST | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/conflicts/:id/rollback` | POST | `core/vortex-server/src/team_api.rs` |
| `/api/memory/conflicts/:id/team` | POST | `core/vortex-server/src/team_api.rs` |
| `/api/memory/dream` | POST | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/dreams` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/export` | GET | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/graph` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/graph/history` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/import` | POST | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/media` | POST | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/meta` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/procedures` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/reason` | POST | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/search` | POST | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/stats` | GET | `core/vortex-server/src/memory_api.rs` |
| `/api/memory/tier` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/tier/demote` | POST | `core/vortex-server/src/memory_adv.rs` |
| `/api/memory/workspaces` | GET | `core/vortex-server/src/memory_api.rs` |
| `/api/messages/:id/feedback` | POST | `core/vortex-server/src/observe_api.rs` |
| `/api/models` | GET | `core/vortex-server/src/api.rs` |
| `/api/neural` | GET | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/bootstrap` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/label` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/mode` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/promote` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/rollback` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/:layer/train` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/neural/reranker/dataset` | POST | `core/vortex-server/src/neural_api.rs` |
| `/api/profile` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/repo/index` | POST | `core/vortex-server/src/automation.rs` |
| `/api/research` | GET, POST | `core/vortex-server/src/research_api.rs` |
| `/api/research/:id` | GET | `core/vortex-server/src/research_api.rs` |
| `/api/research/:id/export` | GET | `core/vortex-server/src/research_api.rs` |
| `/api/research/corpus` | POST | `core/vortex-server/src/research_api.rs` |
| `/api/runs` | GET | `core/vortex-server/src/api.rs` |
| `/api/runs/:id/pause` | POST | `core/vortex-server/src/api.rs` |
| `/api/runs/:id/resume` | POST | `core/vortex-server/src/api.rs` |
| `/api/runs/:id/steer` | POST | `core/vortex-server/src/observe_api.rs` |
| `/api/runs/:id/stop` | POST | `core/vortex-server/src/api.rs` |
| `/api/runs/:id/trace` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/runs/:id/workspace` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/selfmodel` | GET | `core/vortex-server/src/memory_adv.rs` |
| `/api/sessions` | GET, POST | `core/vortex-server/src/api.rs` |
| `/api/sessions/:id` | GET, PATCH | `core/vortex-server/src/api.rs` |
| `/api/sessions/:id/fork` | POST | `core/vortex-server/src/api.rs` |
| `/api/sessions/:id/messages` | POST | `core/vortex-server/src/api.rs` |
| `/api/sessions/:id/replay` | GET | `core/vortex-server/src/observe_api.rs` |
| `/api/settings/history` | GET | `core/vortex-server/src/api.rs` |
| `/api/settings/paths` | GET, PUT | `core/vortex-server/src/paths_api.rs` |
| `/api/settings/prompts` | GET, PUT | `core/vortex-server/src/api.rs` |
| `/api/settings/restore` | POST | `core/vortex-server/src/api.rs` |
| `/api/skills` | GET | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/:name` | DELETE | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/:name/export` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/:name/status` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/import` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/proposals` | GET | `core/vortex-server/src/tools_api.rs` |
| `/api/skills/proposals/:id` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/status` | GET | `core/vortex-server/src/api.rs` |
| `/api/tools` | GET | `core/vortex-server/src/tools_api.rs` |
| `/api/tools/:name/call` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/tools/reload` | POST | `core/vortex-server/src/tools_api.rs` |
| `/api/training` | GET | `core/vortex-server/src/training_api.rs` |
| `/api/training/adapters/:id/gate` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/training/adapters/:id/scale` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/training/adapters/:id/shadow` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/training/calibrate` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/training/datasets/:kind` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/training/train` | POST | `core/vortex-server/src/training_api.rs` |
| `/api/uploads` | POST | `core/vortex-server/src/media.rs` |
| `/api/world` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/bias` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/calibration` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/datasets` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/datasets/import` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/datasets/inspect` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/events` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/events/:id` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/fetch` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/follow` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/hypotheses` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/items` | GET | `core/vortex-server/src/world_api.rs` |
| `/api/world/players` | GET, POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/players/:id/predict` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/players/:id/study` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/predictions` | GET, POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/predictions/resolve` | POST | `core/vortex-server/src/world_api.rs` |
| `/api/world/simulate` | POST | `core/vortex-server/src/frontier.rs` |
| `/api/world/summaries` | POST | `core/vortex-server/src/world_api.rs` |
| `/v1/chat/completions` | POST | `core/vortex-server/src/autonomy/openai.rs` |
| `/v1/models` | GET | `core/vortex-server/src/autonomy/openai.rs` |
