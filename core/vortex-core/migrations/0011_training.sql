-- Training en vliegwiel (W13).
CREATE TABLE training_datasets (
    version    TEXT PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('sft','dpo','qa')),
    path       TEXT NOT NULL,
    n          INTEGER NOT NULL,
    dropped    INTEGER NOT NULL DEFAULT 0,
    stats      TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

-- Adapter-registry: basismodel, dataset, instellingen, scores, regressies, status.
CREATE TABLE adapters (
    id          TEXT PRIMARY KEY,
    base_model  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    dataset     TEXT NOT NULL,
    settings    TEXT NOT NULL,
    adapter_dir TEXT,
    gguf_path   TEXT,
    status      TEXT NOT NULL CHECK (status IN ('training','paused','trained','shadow','active','rejected','failed')),
    scores      TEXT NOT NULL DEFAULT '{}',
    regressions TEXT NOT NULL DEFAULT '[]',
    shadow      TEXT NOT NULL DEFAULT '{}',
    checkpoint  TEXT,
    metrics     TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
