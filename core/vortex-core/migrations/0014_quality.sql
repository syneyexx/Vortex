-- Kwaliteit en performance (W17).
-- Semantische antwoordcache (W17.11): alleen informatieve antwoorden zonder tools/bijlagen,
-- geldig zolang model, config en kennis (geheugengeneratie) gelijk blijven.
CREATE TABLE answer_cache (
    id          TEXT PRIMARY KEY,
    norm        TEXT NOT NULL,
    question    TEXT NOT NULL,
    embedding   TEXT NOT NULL,
    answer      TEXT NOT NULL,
    model       TEXT NOT NULL,
    cfg_version TEXT NOT NULL,
    mem_gen     TEXT NOT NULL,
    run_id      TEXT,
    message_id  TEXT,
    hits        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);
CREATE INDEX answer_cache_norm ON answer_cache(norm);

-- Varianten (A/B, schaduw) van prompts, modellen en strategieën (W17.03/W17.05).
CREATE TABLE experiments (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('shadow','ab','metamorphic','positional','ablation','faults','compression')),
    spec       TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('running','done','failed')),
    result     TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
