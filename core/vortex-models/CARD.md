# CARD — `vortex-models`

**Doel:** inference en modelbeheer: registry, Backend-trait + OpenAI-compatibele adapter (llama-server), toolcall-parsers, streaming, lifecycle (laden/opwarmen/draining/wisselen), metingen.
**VAULT-module:** VAULT/modules/inference-modelbeheer.md · **Waves:** W02 (code af; PC-verificatie open), W17

## Publieke API
| Item | Doet |
| --- | --- |
| `Registry::from_config(&ConfigSnapshot)` | `config/models.toml` → servers + modellen; `validate()` geeft álle fouten; `for_role("chat")` |
| `Backend` (trait) | `capabilities`, `health`, `chat`, `chat_stream`, `embed`, `rerank` |
| `OpenAiBackend::new(name, url, model, &declared_caps, parser_id)` | v1-backend; `probe()` leest `/props` en scherpt caps aan (template-contract) |
| `Capabilities::{intersect, check}` | negotiation: niet ondersteund = `Unsupported`, nooit doen alsof |
| `ChatRequest` / `Message` / `ToolSpec` / `ResponseFormat` | verzoek; json_schema/json_object/GBNF, reasoning_effort, stop, seed, logprobs |
| `StreamEvent` | `Text`, `Reasoning`, `ToolCallStreaming` (nog niet uitvoeren), `ToolCallReady` (gevalideerd), `Done{metrics}` |
| `toolcall::{parser_by_id, validate}` | `openai-native/v1`, `hermes-xml/v1`; ongeldig = `InvalidToolCall` |
| `stream::{Utf8Decoder, StopMatcher, ToolArgsAssembler}` | UTF-8 over chunks, stop over fragmenten, tool-args pas bij compleet |
| `ModelServer` | lifecycle per GPU: `load`, `unload` (draining), `switch` (KV save/restore), `begin_request`, `spawn_idle_return` |
| `lifecycle::{llama_args, prefetch_file}` | llama-server-argumenten uit registry; modelbestand voorverwarmen |
| `InferenceMetrics` / `MetricsRecorder` | wacht, TTFT, prefill/decode t/s, inter-token; NaN/inf = fout; `to_event()` |

## Afhankelijk van
`vortex-types`, `vortex-core` (Supervisor/ServiceHandle, EventLog, ConfigSnapshot) · reqwest

## Events
`model.lifecycle.changed` (elke fase), `model.inference.measured` (via `to_event`), `process.supervisor.*`.

## Invarianten
- Toolcall wordt pas `ToolCallReady` na volledige + geldige JSON én schema-check.
- Tijdens `Draining` geen nieuwe verzoeken; lopende maken hun werk af (tot `drain_timeout`).
- Capabilities = registry ∩ backend ∩ wat `/props` meldt.

## Ingangen
`src/openai.rs` (adapter) · `src/lifecycle.rs` · `src/registry.rs` · `src/toolcall.rs` · `src/stream.rs`

## Tests
`cargo test -p vortex-models` — unit (16) + `tests/openai_mock.rs` (9, nep-server in axum) + `tests/lifecycle.rs` (7, test-dubbel `tests/fake_llama_server.py`). **Echte llama-server + modellen: op de PC.**

## One-brain-check
- [x] event log · [ ] workspace (W07) · [ ] geheugen · [ ] metacog (router kiest model: W07/W11) · [ ] GUI Models-pagina (W03)

## W15
`ServerEntry.{managed, launch_args}` (Vortex beheert de llama-server zelf, GPU-bewaker) en `ModelServer::inflight()`.
