-- Auto-apply: which profile fact the user put into a field with this label.
CREATE TABLE apply_memory (
    label_norm TEXT PRIMARY KEY,
    fact_key   TEXT NOT NULL,
    hits       INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
