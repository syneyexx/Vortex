-- Sessies (gesprekken) en berichten. De opslag is eigenaar van sessies (10-ARCHITECTUUR).
CREATE TABLE sessions (
    id                  TEXT PRIMARY KEY,
    title               TEXT NOT NULL,
    workspace_id        TEXT,
    parent_session_id   TEXT REFERENCES sessions(id),
    forked_from_message TEXT,
    archived            INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX sessions_updated ON sessions(updated_at);

CREATE TABLE messages (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL REFERENCES sessions(id),
    ord            INTEGER NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant', 'tool')),
    content        TEXT NOT NULL DEFAULT '',
    reasoning      TEXT NOT NULL DEFAULT '',
    tool_calls     TEXT NOT NULL DEFAULT '[]',
    run_id         TEXT,
    model          TEXT,
    mode           TEXT,
    config_version TEXT,
    status         TEXT NOT NULL CHECK (status IN ('complete', 'streaming', 'failed', 'cancelled')),
    metrics        TEXT,
    created_at     TEXT NOT NULL,
    UNIQUE (session_id, ord)
);
CREATE INDEX messages_run ON messages(run_id);
