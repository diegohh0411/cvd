-- Change feed: replaces SQLite's data_change counters. Each write statement
-- notifies once per affected tenant; Postgres delivers on commit and folds
-- identical payloads within a transaction.
CREATE FUNCTION notify_data_change() RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $$
DECLARE
    tenant uuid;
BEGIN
    IF TG_OP = 'DELETE' THEN
        FOR tenant IN SELECT DISTINCT user_id FROM old_rows LOOP
            PERFORM pg_notify('db_changed', json_build_object('user_id', tenant, 'table', TG_TABLE_NAME)::text);
        END LOOP;
    ELSE
        FOR tenant IN SELECT DISTINCT user_id FROM new_rows LOOP
            PERFORM pg_notify('db_changed', json_build_object('user_id', tenant, 'table', TG_TABLE_NAME)::text);
        END LOOP;
    END IF;
    RETURN NULL;
END;
$$;

DO $$
DECLARE
    t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'profile', 'entry', 'bullet', 'skill', 'entry_skill', 'job_description',
        'match_run', 'match_score', 'cv', 'settings', 'secrets', 'job_status',
        'company', 'company_alias', 'scan_run', 'processed_email', 'apply_memory',
        'job_view'
    ] LOOP
        EXECUTE format('CREATE TRIGGER %I AFTER INSERT ON %I REFERENCING NEW TABLE AS new_rows
            FOR EACH STATEMENT EXECUTE FUNCTION notify_data_change()', t || '_insert_change', t);
        EXECUTE format('CREATE TRIGGER %I AFTER UPDATE ON %I REFERENCING NEW TABLE AS new_rows
            FOR EACH STATEMENT EXECUTE FUNCTION notify_data_change()', t || '_update_change', t);
        EXECUTE format('CREATE TRIGGER %I AFTER DELETE ON %I REFERENCING OLD TABLE AS old_rows
            FOR EACH STATEMENT EXECUTE FUNCTION notify_data_change()', t || '_delete_change', t);
    END LOOP;
END;
$$;
