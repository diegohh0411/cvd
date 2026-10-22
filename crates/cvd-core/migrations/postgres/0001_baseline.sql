-- Supabase provides auth.users. For plain Postgres tests, bootstrap it first.
-- Supabase may already install citext in its extensions schema.
SET LOCAL search_path = public, extensions;
CREATE EXTENSION IF NOT EXISTS citext;
-- A marker prevents deleted defaults from being re-created on later requests.
CREATE TABLE store_user (
    user_id uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE
);
CREATE TABLE profile (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint,
    name text NOT NULL DEFAULT '',
    headline text NOT NULL DEFAULT '',
    email text NOT NULL DEFAULT '',
    phone text NOT NULL DEFAULT '',
    location text NOT NULL DEFAULT '',
    links_json jsonb NOT NULL DEFAULT '[]',
    summary text NOT NULL DEFAULT '',
    PRIMARY KEY (user_id, id)
);
CREATE INDEX profile_tenant ON profile(user_id);
CREATE TABLE entry (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL,
    title citext NOT NULL,
    org citext NOT NULL DEFAULT '',
    location text NOT NULL DEFAULT '',
    start_date text NOT NULL,
    end_date text,
    description text NOT NULL DEFAULT '',
    sort_hint bigint NOT NULL DEFAULT 0,
    archived boolean NOT NULL DEFAULT false,
    UNIQUE (user_id, id),
    CHECK (kind IN ('experience','project','education','volunteer'))
);
CREATE INDEX entry_tenant ON entry(user_id);
CREATE TABLE bullet (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    entry_id bigint NOT NULL,
    text text NOT NULL,
    position bigint NOT NULL,
    UNIQUE (user_id, id)
);
CREATE INDEX bullet_tenant ON bullet(user_id);
CREATE TABLE skill (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name citext NOT NULL,
    category text NOT NULL DEFAULT '',
    UNIQUE (user_id, id),
    UNIQUE (user_id, name)
);
CREATE INDEX skill_tenant ON skill(user_id);
CREATE TABLE entry_skill (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    entry_id bigint NOT NULL,
    skill_id bigint NOT NULL,
    PRIMARY KEY (user_id, entry_id, skill_id)
);
CREATE INDEX entry_skill_tenant ON entry_skill(user_id);
CREATE TABLE job_description (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    title text NOT NULL,
    company text NOT NULL DEFAULT '',
    raw_text text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    url text NOT NULL DEFAULT '',
    status_id bigint,
    company_id bigint,
    UNIQUE (user_id, id)
);
CREATE INDEX job_description_tenant ON job_description(user_id);
CREATE TABLE match_run (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    jd_id bigint NOT NULL,
    model_id text NOT NULL,
    status text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    company_researched_at timestamptz,
    UNIQUE (user_id, id),
    CHECK (status IN ('running','done','cancelled','failed'))
);
CREATE INDEX match_run_tenant ON match_run(user_id);
CREATE TABLE match_score (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL,
    target_type text NOT NULL,
    target_id bigint NOT NULL,
    question text NOT NULL,
    probabilities_json jsonb NOT NULL,
    score double precision NOT NULL,
    CHECK (target_type IN ('entry','bullet','skill','meta'))
);
CREATE INDEX match_score_tenant ON match_score(user_id);
CREATE TABLE cv (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    jd_id bigint NOT NULL,
    run_id bigint,
    template text NOT NULL,
    page_budget bigint NOT NULL,
    selection_json jsonb NOT NULL,
    pdf_path text,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, id)
);
CREATE INDEX cv_tenant ON cv(user_id);
CREATE TABLE settings (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    key text,
    value text NOT NULL,
    PRIMARY KEY (user_id, key)
);
CREATE INDEX settings_tenant ON settings(user_id);
CREATE TABLE job_status (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name citext NOT NULL,
    color text NOT NULL DEFAULT 'gray',
    position bigint NOT NULL DEFAULT 0,
    UNIQUE (user_id, id),
    UNIQUE (user_id, name),
    CHECK (color IN ('gray','brown','orange','yellow','green','blue','purple','pink','red'))
);
CREATE INDEX job_status_tenant ON job_status(user_id);
CREATE TABLE secrets (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    provider text,
    api_key text NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (user_id, provider)
);
CREATE INDEX secrets_tenant ON secrets(user_id);
CREATE TABLE company (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    normalized_name text NOT NULL,
    domain text,
    summary text NOT NULL DEFAULT '',
    industry text NOT NULL DEFAULT '',
    size text NOT NULL DEFAULT '',
    stage text NOT NULL DEFAULT '',
    headquarters text NOT NULL DEFAULT '',
    remote_policy text NOT NULL DEFAULT '',
    products text NOT NULL DEFAULT '',
    tech_stack_json jsonb NOT NULL DEFAULT '[]',
    sources_json jsonb NOT NULL DEFAULT '[]',
    status text NOT NULL DEFAULT 'pending',
    researched_at timestamptz,
    model_id text NOT NULL DEFAULT '',
    fail_count bigint NOT NULL DEFAULT 0,
    last_error text NOT NULL DEFAULT '',
    retry_after timestamptz,
    edited_at timestamptz,
    UNIQUE (user_id, id),
    UNIQUE (user_id, domain),
    UNIQUE (user_id, normalized_name),
    CHECK (status IN ('pending','ok','failed'))
);
CREATE INDEX company_tenant ON company(user_id);
CREATE TABLE company_alias (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    normalized_name text,
    company_id bigint NOT NULL,
    PRIMARY KEY (user_id, normalized_name)
);
CREATE INDEX company_alias_tenant ON company_alias(user_id);
CREATE TABLE scan_run (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    started_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at timestamptz,
    status text NOT NULL,
    error text,
    checked bigint NOT NULL DEFAULT 0,
    failed bigint NOT NULL DEFAULT 0,
    source text NOT NULL DEFAULT 'manual',
    UNIQUE (user_id, id),
    CHECK (status IN ('running','done','failed')),
    CHECK (source IN ('watch','recheck','manual'))
);
CREATE INDEX scan_run_tenant ON scan_run(user_id);
CREATE TABLE processed_email (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    message_id text,
    run_id bigint NOT NULL,
    processed_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    received_at timestamptz NOT NULL,
    subject text NOT NULL,
    sender text NOT NULL,
    matched_job_id bigint,
    previous_status_id bigint,
    suggested_status_id bigint,
    confidence double precision,
    summary text,
    outcome text NOT NULL,
    job_related double precision,
    body text,
    read_at timestamptz,
    undone_at timestamptz,
    PRIMARY KEY (user_id, message_id),
    CHECK (outcome IN ('ignored','no_change','applied','pending_review','dismissed'))
);
CREATE INDEX processed_email_tenant ON processed_email(user_id);
CREATE TABLE apply_memory (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    label_norm text,
    fact_key text NOT NULL,
    hits bigint NOT NULL DEFAULT 1,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, label_norm)
);
CREATE INDEX apply_memory_tenant ON apply_memory(user_id);
CREATE TABLE job_view (
    user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    layout text NOT NULL DEFAULT 'table',
    filters jsonb NOT NULL DEFAULT '[]',
    sorts jsonb NOT NULL DEFAULT '[]',
    position bigint NOT NULL DEFAULT 0,
    UNIQUE (user_id, id),
    CHECK (layout IN ('table', 'board'))
);
CREATE INDEX job_view_tenant ON job_view(user_id);

ALTER TABLE bullet ADD FOREIGN KEY (user_id, entry_id) REFERENCES entry(user_id, id) ON DELETE CASCADE;
ALTER TABLE entry_skill ADD FOREIGN KEY (user_id, skill_id) REFERENCES skill(user_id, id) ON DELETE CASCADE;
ALTER TABLE entry_skill ADD FOREIGN KEY (user_id, entry_id) REFERENCES entry(user_id, id) ON DELETE CASCADE;
ALTER TABLE job_description ADD FOREIGN KEY (user_id, company_id) REFERENCES company(user_id, id) ON DELETE SET NULL (company_id);
ALTER TABLE job_description ADD FOREIGN KEY (user_id, status_id) REFERENCES job_status(user_id, id) ON DELETE SET NULL (status_id);
ALTER TABLE match_run ADD FOREIGN KEY (user_id, jd_id) REFERENCES job_description(user_id, id) ON DELETE CASCADE;
ALTER TABLE match_score ADD FOREIGN KEY (user_id, run_id) REFERENCES match_run(user_id, id) ON DELETE CASCADE;
ALTER TABLE cv ADD FOREIGN KEY (user_id, run_id) REFERENCES match_run(user_id, id) ON DELETE SET NULL (run_id);
ALTER TABLE cv ADD FOREIGN KEY (user_id, jd_id) REFERENCES job_description(user_id, id) ON DELETE CASCADE;
ALTER TABLE company_alias ADD FOREIGN KEY (user_id, company_id) REFERENCES company(user_id, id) ON DELETE CASCADE;
ALTER TABLE processed_email ADD FOREIGN KEY (user_id, suggested_status_id) REFERENCES job_status(user_id, id) ON DELETE SET NULL (suggested_status_id);
ALTER TABLE processed_email ADD FOREIGN KEY (user_id, previous_status_id) REFERENCES job_status(user_id, id) ON DELETE SET NULL (previous_status_id);
ALTER TABLE processed_email ADD FOREIGN KEY (user_id, matched_job_id) REFERENCES job_description(user_id, id) ON DELETE SET NULL (matched_job_id);
ALTER TABLE processed_email ADD FOREIGN KEY (user_id, run_id) REFERENCES scan_run(user_id, id) ON DELETE CASCADE;
ALTER TABLE store_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE store_user FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON store_user
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE profile ENABLE ROW LEVEL SECURITY;
ALTER TABLE profile FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON profile
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE entry ENABLE ROW LEVEL SECURITY;
ALTER TABLE entry FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON entry
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE bullet ENABLE ROW LEVEL SECURITY;
ALTER TABLE bullet FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON bullet
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE skill ENABLE ROW LEVEL SECURITY;
ALTER TABLE skill FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON skill
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE entry_skill ENABLE ROW LEVEL SECURITY;
ALTER TABLE entry_skill FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON entry_skill
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE job_description ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_description FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON job_description
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE match_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_run FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON match_run
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE match_score ENABLE ROW LEVEL SECURITY;
ALTER TABLE match_score FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON match_score
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE cv ENABLE ROW LEVEL SECURITY;
ALTER TABLE cv FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON cv
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON settings
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE job_status ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_status FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON job_status
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE secrets FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON secrets
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE company ENABLE ROW LEVEL SECURITY;
ALTER TABLE company FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON company
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE company_alias ENABLE ROW LEVEL SECURITY;
ALTER TABLE company_alias FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON company_alias
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE scan_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE scan_run FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON scan_run
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE processed_email ENABLE ROW LEVEL SECURITY;
ALTER TABLE processed_email FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON processed_email
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE apply_memory ENABLE ROW LEVEL SECURITY;
ALTER TABLE apply_memory FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON apply_memory
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
ALTER TABLE job_view ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_view FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant ON job_view
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

CREATE INDEX bullet_entry ON bullet(user_id, entry_id, position);
CREATE INDEX match_score_run ON match_score(user_id, run_id, id);
CREATE INDEX processed_email_outcome ON processed_email(user_id, outcome);
CREATE INDEX processed_email_read ON processed_email(user_id, read_at);
