-- Wereldkennis (W14).
CREATE TABLE world_sources (
    name       TEXT PRIMARY KEY,
    last_fetch TEXT,
    last_ok    TEXT,
    items      INTEGER NOT NULL DEFAULT 0,
    errors     INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE TABLE world_items (
    id         TEXT PRIMARY KEY,
    source     TEXT NOT NULL,
    kind       TEXT NOT NULL,
    title      TEXT NOT NULL,
    summary    TEXT NOT NULL DEFAULT '',
    url        TEXT,
    published  TEXT,
    fetched_at TEXT NOT NULL,
    lang       TEXT,
    country    TEXT,
    extra      TEXT NOT NULL DEFAULT '{}',
    event_id   TEXT
);
CREATE INDEX world_items_pub ON world_items(published);
CREATE INDEX world_items_event ON world_items(event_id);

CREATE TABLE world_events (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    first_seen  TEXT NOT NULL,
    last_seen   TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('reported','confirmed')),
    n_items     INTEGER NOT NULL,
    groups      TEXT NOT NULL DEFAULT '[]',
    region      TEXT,
    theme       TEXT,
    entities    TEXT NOT NULL DEFAULT '[]',
    memory_id   TEXT
);
CREATE INDEX world_events_seen ON world_events(last_seen);

CREATE TABLE predictions (
    id         TEXT PRIMARY KEY,
    claim      TEXT NOT NULL,
    probability REAL NOT NULL,
    criteria   TEXT NOT NULL,
    resolve_by TEXT NOT NULL,
    actor      TEXT,
    origin     TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('open','true','false','unresolvable')),
    evidence   TEXT,
    created_at TEXT NOT NULL,
    resolved_at TEXT
);

CREATE TABLE hypotheses (
    id         TEXT PRIMARY KEY,
    cause      TEXT NOT NULL,
    effect     TEXT NOT NULL,
    confidence REAL NOT NULL,
    support    INTEGER NOT NULL DEFAULT 1,
    contra     INTEGER NOT NULL DEFAULT 0,
    last_seen  TEXT NOT NULL,
    UNIQUE (cause, effect)
);

CREATE TABLE followed_sites (
    url            TEXT PRIMARY KEY,
    interval_hours REAL NOT NULL,
    last_fetch     TEXT,
    content_hash   TEXT,
    changes        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE players (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    sector  TEXT NOT NULL,
    kind    TEXT NOT NULL CHECK (kind IN ('person','organization')),
    sources TEXT NOT NULL DEFAULT '[]',
    notes   TEXT NOT NULL DEFAULT ''
);

CREATE TABLE hf_catalog (
    id          TEXT PRIMARY KEY,
    source      TEXT NOT NULL,
    schema      TEXT NOT NULL,
    mapping     TEXT NOT NULL,
    destination TEXT NOT NULL,
    license     TEXT,
    rows        INTEGER NOT NULL DEFAULT 0,
    kept        INTEGER NOT NULL DEFAULT 0,
    deduped     INTEGER NOT NULL DEFAULT 0,
    decontaminated INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL,
    output      TEXT,
    version     TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
