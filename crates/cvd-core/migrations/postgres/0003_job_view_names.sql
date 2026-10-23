-- Keep the already-shipped baseline checksum intact. Saved view names, like
-- status and skill names, must be unique within a tenant, not across tenants.
ALTER TABLE job_view ADD CONSTRAINT job_view_user_name_key UNIQUE (user_id, name);
