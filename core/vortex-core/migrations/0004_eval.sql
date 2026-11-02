-- Eval-resultaten per versie + feedback (W06).
CREATE TABLE eval_runs (
    id          TEXT PRIMARY KEY,
    suite       TEXT NOT NULL,
    version     TEXT NOT NULL,     -- config-versie + build + model
    label       TEXT,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    n           INTEGER NOT NULL DEFAULT 0,
    passed      INTEGER NOT NULL DEFAULT 0,
    skipped     INTEGER NOT NULL DEFAULT 0,
    score       REAL,
    metrics     TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX eval_runs_suite ON eval_runs(suite, started_at);

CREATE TABLE eval_results (
    run_id     TEXT NOT NULL REFERENCES eval_runs(id) ON DELETE CASCADE,
    task_id    TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('pass','fail','skip','error')),
    score      REAL NOT NULL,
    latency_ms INTEGER,
    output     TEXT,
    detail     TEXT,
    PRIMARY KEY (run_id, task_id)
);

CREATE TABLE feedback (
    id         TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    run_id     TEXT,
    rating     TEXT NOT NULL CHECK (rating IN ('good','bad','correction')),
    correction TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX feedback_msg ON feedback(message_id);
