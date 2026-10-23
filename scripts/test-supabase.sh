#!/usr/bin/env bash
# Apply the embedded SQLx migrations to a fresh, real Supabase database.
# Requires the Supabase CLI and a Docker-compatible runtime (DOCKER_HOST for Podman).
set -euo pipefail
cd "$(dirname "$0")/.."
repo="$PWD"
# Use the runtime the Supabase CLI talks to: Docker when its daemon is up (CI runners
# also ship Podman, which can't see Docker's containers), otherwise Podman via DOCKER_HOST.
if docker info >/dev/null 2>&1; then
  engine=docker
else
  engine=podman
fi
workdir=$(mktemp -d)
cleanup() {
  supabase stop --workdir "$workdir" --no-backup >/dev/null
  rm -rf "$workdir"
}
trap cleanup EXIT
supabase init --workdir "$workdir" --yes
project_id="cvd-schema-test-$$"
# Keep this separate from any developer's Supabase project and its data.
sed "s/^project_id = .*/project_id = \"$project_id\"/" \
  "$workdir/supabase/config.toml" > "$workdir/config.toml"
mv "$workdir/config.toml" "$workdir/supabase/config.toml"
# The DB image includes Supabase's auth schema; other services aren't needed for
# store contracts. In particular, do not replace auth.users with a test double.
supabase start --workdir "$workdir" \
  --exclude gotrue,realtime,storage-api,imgproxy,kong,mailpit,postgrest,postgres-meta,studio,edge-runtime,logflare,vector,supavisor
container="supabase_db_$project_id"
"$engine" exec -i "$container" psql -v ON_ERROR_STOP=1 -U postgres -d postgres <<'SQL'
CREATE ROLE cvd_app LOGIN PASSWORD 'cvd-test' NOSUPERUSER NOBYPASSRLS;
GRANT USAGE ON SCHEMA public, extensions TO cvd_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO cvd_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE ON SEQUENCES TO cvd_app;
SQL
port=$("$engine" port "$container" 5432/tcp)
port=${port##*:}
export CVD_TEST_POSTGRES_ADMIN_URL="postgres://postgres:postgres@127.0.0.1:$port/postgres"
export CVD_TEST_POSTGRES_URL="postgres://cvd_app:cvd-test@127.0.0.1:$port/postgres"
cd "$repo"
cargo test -p cvd-core --features postgres-tests store:: "$@"
