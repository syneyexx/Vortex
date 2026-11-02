-- Geheugen gevorderd (W12).
-- Afgeleide feiten (graph-redeneren): welke regel, uit welke edges.
ALTER TABLE edges ADD COLUMN derived INTEGER NOT NULL DEFAULT 0;
ALTER TABLE edges ADD COLUMN rule TEXT;
CREATE INDEX edges_rel ON edges(rel);
CREATE TABLE edge_support (
    derived_edge TEXT NOT NULL,
    from_edge    TEXT NOT NULL,
    PRIMARY KEY (derived_edge, from_edge)
);

-- Tiering (SSD/HDD): koude blokken met zstd; de herinnering blijft als stub op de SSD.
ALTER TABLE memories ADD COLUMN tier TEXT NOT NULL DEFAULT 'hot' CHECK (tier IN ('hot','cold'));
CREATE TABLE cold_blocks (
    id         TEXT PRIMARY KEY,
    path       TEXT NOT NULL,
    n          INTEGER NOT NULL,
    bytes      INTEGER NOT NULL,
    centroid   BLOB NOT NULL,
    created_at TEXT NOT NULL,
    last_read  TEXT
);
CREATE TABLE cold_members (
    memory_id TEXT PRIMARY KEY,
    block_id  TEXT NOT NULL REFERENCES cold_blocks(id),
    slot      INTEGER NOT NULL
);
CREATE INDEX cold_members_block ON cold_members(block_id);

-- Dromen (consolidatie in idle): verslag per run.
CREATE TABLE dream_runs (
    id         TEXT PRIMARY KEY,
    report     TEXT NOT NULL,
    created_at TEXT NOT NULL
);
