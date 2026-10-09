CREATE TABLE company (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    normalized_name  TEXT NOT NULL UNIQUE,
    domain           TEXT UNIQUE,
    summary          TEXT NOT NULL DEFAULT '',
    industry         TEXT NOT NULL DEFAULT '',
    size             TEXT NOT NULL DEFAULT '',
    stage            TEXT NOT NULL DEFAULT '',
    headquarters     TEXT NOT NULL DEFAULT '',
    remote_policy    TEXT NOT NULL DEFAULT '',
    products         TEXT NOT NULL DEFAULT '',
    tech_stack_json  TEXT NOT NULL DEFAULT '[]',
    sources_json     TEXT NOT NULL DEFAULT '[]',
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending','ok','failed')),
    researched_at    TEXT,
    model_id         TEXT NOT NULL DEFAULT '',
    fail_count       INTEGER NOT NULL DEFAULT 0,
    last_error       TEXT NOT NULL DEFAULT '',
    retry_after      TEXT,
    edited_at        TEXT
);

CREATE TABLE company_alias (
    normalized_name TEXT PRIMARY KEY,
    company_id      INTEGER NOT NULL REFERENCES company(id) ON DELETE CASCADE
);

ALTER TABLE job_description ADD COLUMN company_id INTEGER REFERENCES company(id) ON DELETE SET NULL;
ALTER TABLE match_run ADD COLUMN company_researched_at TEXT;
