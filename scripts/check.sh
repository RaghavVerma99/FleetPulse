#!/usr/bin/env bash
# Run the same gates as CI: formatting, lints, tests.
# Usage: ./scripts/check.sh
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# DB-backed tests are opt-in here; CI runs them with a Postgres service.
# Locally: TEST_DATABASE_URL=postgres://... bash scripts/check-db.sh
