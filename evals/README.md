# evals/

- `sets/*.jsonl` — eval-suites (één taak per regel). Checks: `contains`, `contains_any`, `not_contains`, `regex`, `number`, `python_tests`, `json_fields`, `cites_source`, `admits_unknown`, `conflicts_at_least`. `requires: "W08"` = overgeslagen tot die wave er is (eerlijk "skip").
- `retrieval.json` — retrieval-eval: corpus + vragen (recall@k, MRR, irrelevant@5, geen-antwoord).
- Draaien: `vortex eval` (CLI) of de knop op de Evals-pagina. Resultaten per versie in de database; `vortex eval --compare` toont het verschil met de vorige run.
- Taken nooit in trainingsdata of geheugen (lekkagecontrole: `vortex eval --leakage`).
