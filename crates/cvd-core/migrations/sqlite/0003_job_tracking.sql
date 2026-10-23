CREATE TABLE job_status (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL COLLATE NOCASE UNIQUE,
    color    TEXT NOT NULL DEFAULT 'gray'
             CHECK (color IN ('gray','brown','orange','yellow','green','blue','purple','pink','red')),
    position INTEGER NOT NULL DEFAULT 0
);

INSERT INTO job_status (name, color, position) VALUES
    ('Bookmarked',   'blue',   0),
    ('Applied',      'yellow', 1),
    ('Interviewing', 'purple', 2),
    ('Offer',        'green',  3);

ALTER TABLE job_description ADD COLUMN url TEXT NOT NULL DEFAULT '';
ALTER TABLE job_description ADD COLUMN status_id INTEGER REFERENCES job_status(id) ON DELETE SET NULL;
