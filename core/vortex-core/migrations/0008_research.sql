-- Research + crawler (W10).
-- Frontier per crawltaak: wat nog opgehaald moet worden, met score (focused crawling).
CREATE TABLE crawl_frontier (
    task_id   TEXT NOT NULL,
    url       TEXT NOT NULL,
    depth     INTEGER NOT NULL,
    score     REAL NOT NULL,
    status    TEXT NOT NULL CHECK (status IN ('queued','fetched','skipped','failed')),
    parent    TEXT,
    anchor    TEXT,
    added_at  TEXT NOT NULL,
    PRIMARY KEY (task_id, url)
);
CREATE INDEX crawl_frontier_next ON crawl_frontier(task_id, status, score DESC);

-- Opgehaalde pagina's (cache): ruwe HTML + tekst op de koude schijf.
CREATE TABLE crawl_pages (
    url          TEXT PRIMARY KEY,
    final_url    TEXT NOT NULL,
    title        TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    simhash      TEXT NOT NULL,
    raw_path     TEXT,
    text_path    TEXT,
    status       INTEGER NOT NULL,
    bytes        INTEGER NOT NULL,
    fetched_at   TEXT NOT NULL
);

-- Bronbetrouwbaarheid per domein (leert van research-uitkomsten).
CREATE TABLE domain_stats (
    domain      TEXT PRIMARY KEY,
    fetched     INTEGER NOT NULL DEFAULT 0,
    useful      INTEGER NOT NULL DEFAULT 0,
    supported   INTEGER NOT NULL DEFAULT 0,
    contradicted INTEGER NOT NULL DEFAULT 0,
    reliability REAL NOT NULL DEFAULT 0.5,
    updated_at  TEXT NOT NULL
);

-- Research-runs, deelvragen en records (live in de GUI, exporteerbaar).
CREATE TABLE research_runs (
    id          TEXT PRIMARY KEY,
    run_id      TEXT,
    question    TEXT NOT NULL,
    mode        TEXT NOT NULL,
    status      TEXT NOT NULL,
    report      TEXT,
    report_path TEXT,
    plan        TEXT,
    sources     TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE research_records (
    id          TEXT PRIMARY KEY,
    research_id TEXT NOT NULL,
    subquestion INTEGER,
    kind        TEXT NOT NULL,
    data        TEXT NOT NULL,
    source_url  TEXT,
    passage     TEXT,
    status      TEXT NOT NULL DEFAULT 'unverified',
    memory_id   TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX research_records_run ON research_records(research_id);
