-- The old registry references point to files CVD no longer serves. Preserve all
-- remote selections and all downloaded files, and leave a notice until dismissed.
INSERT INTO settings (key, value)
SELECT 'ui.local_server_migration', '1'
WHERE EXISTS (
  SELECT 1 FROM settings WHERE key IN ('active_model', 'research_model')
  AND (instr(value, ':') = 0 OR value LIKE 'local:%')
)
ON CONFLICT(key) DO UPDATE SET value = '1';
DELETE FROM settings WHERE key IN ('active_model', 'research_model')
AND (instr(value, ':') = 0 OR value LIKE 'local:%');
