-- Autonomie (W15): taakqueue, projecten, triggers, suggesties, briefings, backups.
CREATE TABLE projects (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    goal       TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL CHECK (status IN ('active','paused','done')),
    workspace  TEXT,
    -- Doorlopende notities/staat over sessies heen (multi-session continuity).
    notes      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE tasks (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    kind         TEXT NOT NULL,
    params       TEXT NOT NULL DEFAULT '{}',
    priority     INTEGER NOT NULL DEFAULT 3,
    status       TEXT NOT NULL CHECK (status IN ('queued','running','done','failed','cancelled','paused')),
    depends_on   TEXT NOT NULL DEFAULT '[]',
    project_id   TEXT,
    needs_idle   INTEGER NOT NULL DEFAULT 1,
    not_before   TEXT,
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    progress     REAL NOT NULL DEFAULT 0,
    result       TEXT,
    error        TEXT,
    run_id       TEXT,
    origin       TEXT NOT NULL DEFAULT 'user',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX tasks_status ON tasks(status, priority, created_at);

CREATE TABLE triggers (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    -- cron ("m h dom mon dow", lokale tijd) | interval (minuten) | event (kind-patroon)
    kind       TEXT NOT NULL CHECK (kind IN ('cron','interval','event')),
    spec       TEXT NOT NULL,
    task       TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    last_fired TEXT,
    last_seq   INTEGER NOT NULL DEFAULT 0,
    fired      INTEGER NOT NULL DEFAULT 0,
    builtin    INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE suggestions (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    text       TEXT NOT NULL,
    action     TEXT,
    status     TEXT NOT NULL CHECK (status IN ('new','accepted','dismissed')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE briefings (
    day        TEXT PRIMARY KEY,
    markdown   TEXT NOT NULL,
    data       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE backups (
    id          TEXT PRIMARY KEY,
    path        TEXT NOT NULL,
    bytes       INTEGER NOT NULL,
    tier        TEXT NOT NULL,
    verified    INTEGER NOT NULL DEFAULT 0,
    verify_info TEXT,
    created_at  TEXT NOT NULL
);

-- OpenAI-compatibele API (W15.07): gesprek van een client → Vortex-sessie (eigen geschiedenis).
CREATE TABLE api_sessions (
    key        TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
