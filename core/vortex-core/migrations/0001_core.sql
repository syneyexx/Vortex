-- Kern-schema v1. Nooit wijzigen na gebruik: nieuwe migratie toevoegen.

-- Event log: append-only (triggers blokkeren UPDATE/DELETE).
CREATE TABLE events (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    id             TEXT NOT NULL UNIQUE,
    kind           TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    ts             TEXT NOT NULL,
    run_id         TEXT,
    trace_id       TEXT NOT NULL,
    span_id        TEXT,
    provenance     TEXT NOT NULL,
    payload        TEXT NOT NULL
);
CREATE INDEX events_run  ON events(run_id, seq);
CREATE INDEX events_kind ON events(kind, seq);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'event log is append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'event log is append-only'); END;

-- Dispatcher: positie per ontvanger + idempotentie-sleutels.
CREATE TABLE consumer_offsets (
    consumer TEXT PRIMARY KEY,
    last_seq INTEGER NOT NULL
);
CREATE TABLE consumer_processed (
    consumer TEXT NOT NULL,
    key      TEXT NOT NULL,
    ts       TEXT NOT NULL,
    PRIMARY KEY (consumer, key)
);

-- Runs met optimistic concurrency (version).
CREATE TABLE runs (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    priority        INTEGER NOT NULL,
    state           TEXT NOT NULL,
    version         INTEGER NOT NULL,
    parent_id       TEXT REFERENCES runs(id),
    trace_id        TEXT NOT NULL,
    config_version  TEXT,
    resumable       INTEGER NOT NULL DEFAULT 0,
    stop_status     TEXT,
    deadline        TEXT,
    budget          TEXT NOT NULL DEFAULT '{}',
    usage           TEXT NOT NULL DEFAULT '{}',
    input           TEXT NOT NULL DEFAULT 'null',
    output          TEXT,
    error           TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX runs_state  ON runs(state);
CREATE INDEX runs_parent ON runs(parent_id);

CREATE TABLE checkpoints (
    id      TEXT PRIMARY KEY,
    run_id  TEXT NOT NULL REFERENCES runs(id),
    seq     INTEGER NOT NULL,
    ts      TEXT NOT NULL,
    data    TEXT NOT NULL,
    UNIQUE (run_id, seq)
);

-- Effecten: intentie vóór een externe actie, 'done' erna. Na crash: 'intent' = mogelijk uitgevoerd.
CREATE TABLE effects (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL REFERENCES runs(id),
    kind        TEXT NOT NULL CHECK (kind IN ('read', 'local', 'external')),
    description TEXT NOT NULL,
    idempotency_key TEXT,
    state       TEXT NOT NULL CHECK (state IN ('intent', 'done', 'failed')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX effects_run ON effects(run_id);

-- Resource ownership (persistent, zodat na crash bekend is wat vastgehouden werd).
CREATE TABLE resources (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    name        TEXT NOT NULL,
    owner_run   TEXT,
    owner       TEXT NOT NULL,
    detail      TEXT NOT NULL DEFAULT '{}',
    acquired_at TEXT NOT NULL
);

CREATE TABLE config_versions (
    version TEXT PRIMARY KEY,
    hash    TEXT NOT NULL,
    ts      TEXT NOT NULL,
    content TEXT NOT NULL
);
