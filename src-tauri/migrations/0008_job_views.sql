-- Saved views of the Jobs page. filters/sorts are JSON owned by the frontend.
CREATE TABLE job_view (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    layout   TEXT NOT NULL DEFAULT 'table' CHECK (layout IN ('table', 'board')),
    filters  TEXT NOT NULL DEFAULT '[]',
    sorts    TEXT NOT NULL DEFAULT '[]',
    position INTEGER NOT NULL DEFAULT 0
);

INSERT INTO job_view (name, layout, position) VALUES ('All jobs', 'table', 0);
