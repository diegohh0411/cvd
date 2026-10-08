CREATE TABLE secrets (
    provider   TEXT PRIMARY KEY,
    api_key    TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
