-- Test doubles for the Supabase APIs; no network delivery is simulated.
CREATE ROLE authenticated;
CREATE ROLE anon;
CREATE SCHEMA realtime;
CREATE TABLE realtime.messages(topic text, extension text, payload jsonb, event text, private boolean);
ALTER TABLE realtime.messages ENABLE ROW LEVEL SECURITY;
GRANT USAGE ON SCHEMA realtime, auth TO authenticated, anon;
GRANT SELECT, INSERT ON realtime.messages TO authenticated, anon;
CREATE FUNCTION auth.uid() RETURNS uuid LANGUAGE sql STABLE AS $$
    SELECT nullif(current_setting('request.jwt.claim.sub', true), '')::uuid
$$;
CREATE FUNCTION realtime.topic() RETURNS text LANGUAGE sql STABLE AS $$
    SELECT current_setting('realtime.topic', true)
$$;
CREATE FUNCTION realtime.send(payload jsonb, event text, topic text, private boolean)
RETURNS void LANGUAGE sql AS $$
    INSERT INTO realtime.messages VALUES (topic, 'broadcast', payload, event, private)
$$;
