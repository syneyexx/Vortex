-- Geheugen (W05). SQLite = bron van waarheid; vectoren staan hier ook (int8-blobs), zodat
-- schrijven/verwijderen in één transactie gebeurt. Alle zoekindexen zijn afgeleid.
CREATE TABLE workspaces (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE sources (
    id           TEXT PRIMARY KEY,
    uri          TEXT NOT NULL UNIQUE,
    kind         TEXT NOT NULL,
    title        TEXT,
    content_hash TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE memories (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind IN ('short_term','long_term','episodic','semantic','user','project','world','procedural','meta','summary')),
    workspace_id  TEXT REFERENCES workspaces(id),
    session_id    TEXT,
    content       TEXT NOT NULL,
    context       TEXT NOT NULL DEFAULT '',
    summary       TEXT NOT NULL DEFAULT '',
    title         TEXT NOT NULL DEFAULT '',
    source_id     TEXT REFERENCES sources(id),
    locator       TEXT,
    provenance    TEXT NOT NULL,
    importance    REAL NOT NULL,
    salience      REAL NOT NULL DEFAULT 0,
    confidence    REAL NOT NULL DEFAULT 0.7,
    valid_from    TEXT,
    valid_to      TEXT,
    event_time    TEXT,
    access_count  INTEGER NOT NULL DEFAULT 0,
    last_access   TEXT,
    version       INTEGER NOT NULL DEFAULT 1,
    content_hash  TEXT NOT NULL,
    stale         INTEGER NOT NULL DEFAULT 0,
    merged_count  INTEGER NOT NULL DEFAULT 1,
    tags          TEXT NOT NULL DEFAULT '[]',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX memories_ws   ON memories(workspace_id, kind);
CREATE INDEX memories_time ON memories(created_at);
CREATE INDEX memories_hash ON memories(content_hash);
CREATE INDEX memories_src  ON memories(source_id);

-- Volledige tekst (BM25) — external content op rowid.
CREATE VIRTUAL TABLE memories_fts USING fts5(title, content, context, summary, content='memories', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, title, content, context, summary) VALUES (new.rowid, new.title, new.content, new.context, new.summary);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, content, context, summary) VALUES ('delete', old.rowid, old.title, old.content, old.context, old.summary);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, content, context, summary) VALUES ('delete', old.rowid, old.title, old.content, old.context, old.summary);
  INSERT INTO memories_fts(rowid, title, content, context, summary) VALUES (new.rowid, new.title, new.content, new.context, new.summary);
END;

CREATE TABLE memory_versions (
    memory_id  TEXT NOT NULL,
    version    INTEGER NOT NULL,
    content    TEXT NOT NULL,
    reason     TEXT NOT NULL,
    changed_at TEXT NOT NULL,
    PRIMARY KEY (memory_id, version)
);

-- Vectoren (int8 + schaal), per indexgeneratie.
CREATE TABLE index_generations (
    generation INTEGER PRIMARY KEY,
    model      TEXT NOT NULL,
    dims       INTEGER NOT NULL,
    normalized INTEGER NOT NULL,
    active     INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE vectors (
    memory_id  TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL REFERENCES index_generations(generation),
    scale      REAL NOT NULL,
    data       BLOB NOT NULL,
    PRIMARY KEY (memory_id, generation)
);

-- Graph: entiteiten + randen (temporeel).
CREATE TABLE entities (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    norm    TEXT NOT NULL UNIQUE,
    type    TEXT NOT NULL DEFAULT 'thing',
    aliases TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE memory_entities (
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id TEXT NOT NULL REFERENCES entities(id),
    PRIMARY KEY (memory_id, entity_id)
);
CREATE INDEX memory_entities_e ON memory_entities(entity_id);
CREATE TABLE edges (
    id         TEXT PRIMARY KEY,
    src        TEXT NOT NULL,
    rel        TEXT NOT NULL,
    dst        TEXT NOT NULL,
    memory_id  TEXT REFERENCES memories(id) ON DELETE CASCADE,
    valid_from TEXT,
    valid_to   TEXT,
    confidence REAL NOT NULL DEFAULT 0.7,
    created_at TEXT NOT NULL
);
CREATE INDEX edges_src ON edges(src);
CREATE INDEX edges_dst ON edges(dst);

CREATE TABLE claims (
    id        TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    subject   TEXT NOT NULL,
    predicate TEXT NOT NULL,
    object    TEXT NOT NULL,
    polarity  INTEGER NOT NULL DEFAULT 1,
    value     REAL,
    unit      TEXT
);
CREATE INDEX claims_sp ON claims(subject, predicate);

-- Herkomstafhankelijkheden: derived_id is afgeleid van from_id (herinnering of bron).
CREATE TABLE derivations (
    derived_id TEXT NOT NULL,
    from_id    TEXT NOT NULL,
    PRIMARY KEY (derived_id, from_id)
);
CREATE INDEX derivations_from ON derivations(from_id);

CREATE TABLE conflicts (
    id          TEXT PRIMARY KEY,
    a_memory    TEXT NOT NULL,
    b_memory    TEXT NOT NULL,
    reason      TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('open','resolved','disputed')),
    resolution  TEXT,
    created_at  TEXT NOT NULL,
    resolved_at TEXT
);

-- Schrijfacties die wachten op het embeddingmodel (bv. 1060-server nog niet gestart).
CREATE TABLE memory_pending (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    payload    TEXT NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL
);
