-- Storage-agnostic reference to an exported CV: a filesystem path on
-- desktop, an object key on web.
ALTER TABLE cv RENAME COLUMN pdf_path TO file_ref;
