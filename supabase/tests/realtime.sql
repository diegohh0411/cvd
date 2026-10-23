BEGIN;
DO $$ BEGIN
    IF (SELECT count(*) FROM pg_trigger
        WHERE tgname = 'broadcast_store_change' AND NOT tgisinternal) <> 18 THEN
        RAISE EXCEPTION 'Missing store broadcast triggers';
    END IF;
END $$;
INSERT INTO auth.users VALUES ('00000000-0000-0000-0000-000000000001'), ('00000000-0000-0000-0000-000000000002');
TRUNCATE realtime.messages;
SET LOCAL ROLE cvd_app;
SET LOCAL app.user_id = '00000000-0000-0000-0000-000000000001';
INSERT INTO profile(user_id, id, name) VALUES (current_setting('app.user_id')::uuid, 1, 'private name');
UPDATE profile SET name = 'changed private name' WHERE id = 1;
DELETE FROM profile WHERE id = 1;
SAVEPOINT undo_write;
INSERT INTO profile(user_id, id) VALUES (current_setting('app.user_id')::uuid, 1);
ROLLBACK TO undo_write;
RESET ROLE;
DO $$
BEGIN
    IF (SELECT count(*) FROM realtime.messages) <> 3 OR EXISTS (
        SELECT FROM realtime.messages WHERE topic <> 'user:00000000-0000-0000-0000-000000000001'
        OR payload <> '{"table":"profile"}'::jsonb OR event <> 'invalidate' OR NOT private
    ) THEN
        RAISE EXCEPTION 'Incorrect routing, payload, or rollback behavior';
    END IF;
END;
$$;
SET LOCAL ROLE authenticated;
SET LOCAL request.jwt.claim.sub = '00000000-0000-0000-0000-000000000001';
SET LOCAL realtime.topic = 'user:00000000-0000-0000-0000-000000000001';
DO $$ BEGIN
    IF (SELECT count(*) FROM realtime.messages) <> 3 THEN
        RAISE EXCEPTION 'Owner cannot receive broadcasts';
    END IF;
    BEGIN
        INSERT INTO realtime.messages VALUES ('forged', 'broadcast', '{}', 'invalidate', true);
        RAISE EXCEPTION 'Client was allowed to publish';
    EXCEPTION WHEN insufficient_privilege THEN NULL;
    END;
END $$;
SET LOCAL realtime.topic = 'user:00000000-0000-0000-0000-000000000002';
DO $$ BEGIN
    IF EXISTS (SELECT FROM realtime.messages) THEN
        RAISE EXCEPTION 'Cross-tenant subscription allowed';
    END IF;
END $$;
SET LOCAL realtime.topic = 'user:00000000-0000-0000-0000-000000000001';
SET LOCAL request.jwt.claim.sub = '';
DO $$ BEGIN
    IF EXISTS (SELECT FROM realtime.messages) THEN
        RAISE EXCEPTION 'Missing JWT identity allowed';
    END IF;
END $$;
SET LOCAL ROLE anon;
DO $$ BEGIN
    IF EXISTS (SELECT FROM realtime.messages) THEN
        RAISE EXCEPTION 'Anonymous subscription allowed';
    END IF;
END $$;
ROLLBACK;
