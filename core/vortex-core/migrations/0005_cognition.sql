-- Leeragenda (W07.08): kennisgaten, betwiste feiten, zwakke taaktypen.
CREATE TABLE learning_agenda (
    id          TEXT PRIMARY KEY,
    topic       TEXT NOT NULL,
    reason      TEXT NOT NULL CHECK (reason IN ('gap','disputed','weak_task','user')),
    detail      TEXT NOT NULL DEFAULT '',
    priority    REAL NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('open','researching','done','dropped')),
    hits        INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE (topic, reason)
);
