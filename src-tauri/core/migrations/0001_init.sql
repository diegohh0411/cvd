CREATE TABLE profile (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL DEFAULT '',
    headline   TEXT NOT NULL DEFAULT '',
    email      TEXT NOT NULL DEFAULT '',
    phone      TEXT NOT NULL DEFAULT '',
    location   TEXT NOT NULL DEFAULT '',
    links_json TEXT NOT NULL DEFAULT '[]',
    summary    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE entry (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('experience','project','education','volunteer')),
    title       TEXT NOT NULL,
    org         TEXT NOT NULL DEFAULT '',
    location    TEXT NOT NULL DEFAULT '',
    start_date  TEXT NOT NULL,
    end_date    TEXT,
    description TEXT NOT NULL DEFAULT '',
    sort_hint   INTEGER NOT NULL DEFAULT 0,
    archived    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE bullet (
    id       INTEGER PRIMARY KEY,
    entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
    text     TEXT NOT NULL,
    position INTEGER NOT NULL
);
CREATE INDEX bullet_entry ON bullet(entry_id, position);

CREATE TABLE skill (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL COLLATE NOCASE UNIQUE,
    category TEXT NOT NULL DEFAULT ''
);

CREATE TABLE entry_skill (
    entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
    skill_id INTEGER NOT NULL REFERENCES skill(id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, skill_id)
);

CREATE TABLE job_description (
    id         INTEGER PRIMARY KEY,
    title      TEXT NOT NULL,
    company    TEXT NOT NULL DEFAULT '',
    raw_text   TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE match_run (
    id         INTEGER PRIMARY KEY,
    jd_id      INTEGER NOT NULL REFERENCES job_description(id) ON DELETE CASCADE,
    model_id   TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('running','done','cancelled','failed')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE match_score (
    run_id             INTEGER NOT NULL REFERENCES match_run(id) ON DELETE CASCADE,
    target_type        TEXT NOT NULL CHECK (target_type IN ('entry','bullet','skill','meta')),
    target_id          INTEGER NOT NULL,
    question           TEXT NOT NULL,
    probabilities_json TEXT NOT NULL,
    score              REAL NOT NULL
);
CREATE INDEX match_score_run ON match_score(run_id);

CREATE TABLE cv (
    id             INTEGER PRIMARY KEY,
    jd_id          INTEGER NOT NULL REFERENCES job_description(id) ON DELETE CASCADE,
    run_id         INTEGER REFERENCES match_run(id) ON DELETE SET NULL,
    template       TEXT NOT NULL,
    page_budget    INTEGER NOT NULL,
    selection_json TEXT NOT NULL,
    pdf_path       TEXT,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
