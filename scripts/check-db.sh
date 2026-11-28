#!/usr/bin/env bash
# Opt-in: run the DB-backed integration tests against a real Postgres.
# Usage: TEST_DATABASE_URL=postgres://... bash scripts/check-db.sh
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z "${TEST_DATABASE_URL:-${DATABASE_URL:-}}" ]]; then
  echo "set TEST_DATABASE_URL (or DATABASE_URL) to a migrated test database" >&2
  exit 1
fi

cargo test --workspace -- --ignored
