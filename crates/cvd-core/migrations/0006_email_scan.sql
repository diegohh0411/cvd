CREATE TABLE scan_run (
    id INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('running','done','failed')),
    error TEXT,
    checked INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE processed_email (
    message_id TEXT PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES scan_run(id) ON DELETE CASCADE,
    processed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    received_at TEXT NOT NULL,
    subject TEXT NOT NULL,
    sender TEXT NOT NULL,
    matched_job_id INTEGER REFERENCES job_description(id) ON DELETE SET NULL,
    previous_status_id INTEGER REFERENCES job_status(id) ON DELETE SET NULL,
    suggested_status_id INTEGER REFERENCES job_status(id) ON DELETE SET NULL,
    confidence REAL,
    summary TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('ignored','no_change','applied','pending_review','dismissed'))
);

CREATE INDEX processed_email_outcome_idx ON processed_email(outcome);
