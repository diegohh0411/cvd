-- Apply as the Supabase schema owner after Store::migrate_postgres.
BEGIN;
CREATE OR REPLACE FUNCTION public.broadcast_store_change()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    owner_id uuid;
BEGIN
    IF TG_OP = 'DELETE' THEN
        owner_id := OLD.user_id;
    ELSE
        owner_id := NEW.user_id;
    END IF;
    -- Invalidation only: never publish rows, credentials, or profile content.
    PERFORM realtime.send(
        jsonb_build_object('table', TG_TABLE_NAME),
        'invalidate',
        'user:' || owner_id::text,
        true
    );
    RETURN NULL;
END;
$$;
REVOKE ALL ON FUNCTION public.broadcast_store_change() FROM PUBLIC;

DO $$
DECLARE
    table_name text;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'profile', 'entry', 'bullet', 'skill', 'entry_skill',
        'job_description', 'match_run', 'match_score', 'cv', 'settings',
        'job_status', 'secrets', 'company', 'company_alias', 'scan_run',
        'processed_email', 'apply_memory', 'job_view'
    ] LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS broadcast_store_change ON public.%I', table_name);
        EXECUTE format(
            'CREATE TRIGGER broadcast_store_change AFTER INSERT OR UPDATE OR DELETE ON public.%I '
            'FOR EACH ROW EXECUTE FUNCTION public.broadcast_store_change()', table_name
        );
    END LOOP;
END;
$$;

DROP POLICY IF EXISTS cvd_receive_own_changes ON realtime.messages;
CREATE POLICY cvd_receive_own_changes ON realtime.messages
FOR SELECT TO authenticated
USING (
    extension = 'broadcast'
    AND (SELECT realtime.topic()) = 'user:' || (SELECT auth.uid())::text
);
-- No client INSERT policy: clients receive invalidations, never publish them.
COMMIT;
