-- Debatteam + conflict-team (W09).
-- Historie van conflictbeslissingen: vorige waarden per herinnering, zodat elke beslissing
-- terug te draaien is.
CREATE TABLE conflict_history (
    id              TEXT PRIMARY KEY,
    conflict_id     TEXT NOT NULL,
    memory_id       TEXT NOT NULL,
    prev_valid_from TEXT,
    prev_valid_to   TEXT,
    prev_confidence REAL NOT NULL,
    decision        TEXT NOT NULL,
    applied_at      TEXT NOT NULL,
    rolled_back_at  TEXT
);
CREATE INDEX conflict_history_conflict ON conflict_history(conflict_id);
