CREATE TABLE IF NOT EXISTS matches (
    uei TEXT NOT NULL,
    cik TEXT NOT NULL,
    method TEXT NOT NULL CHECK (method = 'legal_name_exact'),
    recipient_name TEXT NOT NULL,
    sec_name TEXT NOT NULL,
    state TEXT NOT NULL,
    as_of_date TEXT NOT NULL,
    deleted_at INTEGER,
    PRIMARY KEY (uei, cik)
) STRICT;

CREATE TABLE IF NOT EXISTS match_runs (
    as_of_date TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    ciks_seen INTEGER NOT NULL,
    recipients_seen INTEGER NOT NULL,
    matches_upserted INTEGER NOT NULL,
    matches_retracted INTEGER NOT NULL
) STRICT;
