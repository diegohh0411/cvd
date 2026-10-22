#!/usr/bin/env bash
# Run the same store contracts against isolated Postgres, using Podman.
set -euo pipefail
cd "$(dirname "$0")/.."
container="cvd-postgres-test-$$"
podman run -d --name "$container" --rm \
  -e POSTGRES_PASSWORD=cvd-test -e POSTGRES_DB=cvd \
  -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
trap 'podman rm -f "$container" >/dev/null' EXIT
ready=false
for _ in {1..30}; do
  if podman exec "$container" pg_isready -U postgres >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 1
done
if [ "$ready" != true ]; then
  podman logs "$container"
  exit 1
fi
podman exec -i "$container" psql -v ON_ERROR_STOP=1 -U postgres -d cvd <<'SQL'
CREATE SCHEMA auth;
CREATE TABLE auth.users(id uuid PRIMARY KEY);
-- Match Supabase, where extensions can already live outside public.
CREATE SCHEMA extensions;
CREATE EXTENSION citext WITH SCHEMA extensions;
CREATE ROLE cvd_app LOGIN PASSWORD 'cvd-test';
GRANT USAGE ON SCHEMA public, extensions TO cvd_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO cvd_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE ON SEQUENCES TO cvd_app;
SQL
port=$(podman port "$container" 5432/tcp)
port=${port##*:}
export CVD_TEST_POSTGRES_ADMIN_URL="postgres://postgres:cvd-test@127.0.0.1:$port/cvd"
export CVD_TEST_POSTGRES_URL="postgres://cvd_app:cvd-test@127.0.0.1:$port/cvd"
cargo test --workspace --features cvd-core/postgres-tests "$@"
