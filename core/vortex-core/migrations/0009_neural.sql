-- Neurale layers (W11): labels per voorbeeld (LM-bootstrap, mens, uitkomst) en trainingsruns.
CREATE TABLE neural_labels (
    layer      TEXT NOT NULL,
    ref        TEXT NOT NULL,
    label      REAL NOT NULL,
    source     TEXT NOT NULL CHECK (source IN ('lm','human','outcome')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (layer, ref)
);

CREATE TABLE neural_runs (
    id         TEXT PRIMARY KEY,
    layer      TEXT NOT NULL,
    dataset    TEXT NOT NULL,
    version    TEXT,
    n_train    INTEGER NOT NULL,
    n_test     INTEGER NOT NULL,
    net        REAL,
    heuristic  REAL,
    decision   TEXT,
    trainer    TEXT,
    created_at TEXT NOT NULL
);
