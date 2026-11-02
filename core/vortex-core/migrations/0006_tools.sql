-- Tools en skills (W08).
-- Idempotentie-ledger: één rij per actie-sleutel (taak-ID). Retry met dezelfde sleutel geeft het
-- opgeslagen resultaat terug i.p.v. de actie opnieuw uit te voeren.
CREATE TABLE tool_ledger (
    key         TEXT PRIMARY KEY,
    tool        TEXT NOT NULL,
    input_hash  TEXT NOT NULL,
    effect      TEXT NOT NULL CHECK (effect IN ('read','local','external')),
    state       TEXT NOT NULL CHECK (state IN ('intent','done','failed')),
    output      TEXT,
    error       TEXT,
    run_id      TEXT,
    attempts    INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX tool_ledger_run ON tool_ledger(run_id);

-- Skill-statistiek: gebruik, succes, status (actief/uitgezet), versie.
CREATE TABLE skill_stats (
    name        TEXT PRIMARY KEY,
    version     TEXT NOT NULL,
    uses        INTEGER NOT NULL DEFAULT 0,
    successes   INTEGER NOT NULL DEFAULT 0,
    failures    INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','disabled')),
    updated_at  TEXT NOT NULL
);

-- Skill-voorstellen uit geslaagde traces (W08.14).
CREATE TABLE skill_proposals (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    signature   TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    body        TEXT NOT NULL,
    source_run  TEXT,
    seen        INTEGER NOT NULL DEFAULT 1,
    status      TEXT NOT NULL CHECK (status IN ('proposed','accepted','rejected')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
