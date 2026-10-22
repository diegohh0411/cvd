ALTER TABLE processed_email ADD COLUMN job_related REAL;
ALTER TABLE processed_email ADD COLUMN body TEXT;
ALTER TABLE processed_email ADD COLUMN read_at TEXT;
ALTER TABLE processed_email ADD COLUMN undone_at TEXT;

-- Rows from before the inbox count as read, so upgrading does not flood the badge.
UPDATE processed_email SET read_at = processed_at;

ALTER TABLE scan_run ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'
    CHECK (source IN ('watch','recheck','manual'));

CREATE INDEX processed_email_read_idx ON processed_email(read_at);
