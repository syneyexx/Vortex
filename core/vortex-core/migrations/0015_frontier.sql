-- Frontier (W18): experimentsoorten vrijgeven (prompt-optimalisatie, assoc, continual, …).
CREATE TABLE experiments_new (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    spec       TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('running','done','failed')),
    result     TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO experiments_new SELECT id, kind, spec, status, result, created_at, updated_at FROM experiments;
DROP TABLE experiments;
ALTER TABLE experiments_new RENAME TO experiments;
