# FleetPulse

FleetPulse is a Rust backend for connected-vehicle telemetry. It ingests device batches, persists readings in PostgreSQL, and exposes operator query APIs with idempotent writes, typed errors, and operational health checks.

Built as a portfolio project for the Bytebeam Backend Engineer (SDE 1) role.

## Status

| Milestone | State | Notes |
|---|---|---|
| M1 Skeleton | Complete | Config, migrations, DB pool, tracing, health, graceful shutdown |
| M2 Telemetry ingest | Complete | Device auth, validation, atomic batch insert, `batch_id` dedupe |
| M3 Query APIs | Complete | Device list/detail, readings range + cursor, latest-per-metric |
| M4 Quality | Planned | Seed/demo polish, DB-backed CI run, reproducible walkthrough |
| M5 Alerts | Optional | Inline threshold evaluation |

> DB-backed integration tests exist but are `#[ignore]` without `TEST_DATABASE_URL` / `DATABASE_URL`. On this machine, Postgres is not configured; `scripts/check.sh` passes for fmt/clippy/unit/router tests.

## System design

### Runtime shape

```text
Device / simulator
  │  POST /v1/telemetry  (X-Device-Key)
  ▼
Axum API (fleetpulse-api)
  ├─ request-id + tracing
  ├─ auth extractors (device/operator)
  ├─ validation + typed ApiError
  └─ store calls (no SQL in handlers)
        │
        ▼
PostgreSQL (source of truth)
  devices, api_keys, telemetry_batches, readings
```

### Design choices

| Concern | Implementation | Why |
|---|---|---|
| Idempotent ingest | Client-supplied `batch_id`; `INSERT ... ON CONFLICT (id) DO NOTHING` | Safe retries over unreliable device networks |
| Atomicity | Batch row + readings + device `last_seen` in one DB transaction | No partial batch state |
| Auth | `X-Device-Key` verified against Argon2id hashes in `api_keys`; `X-Operator-Key` static env key | Separates device and operator trust boundaries (ADR-013) |
| Validation | `validator` + manual checks (finite value, future skew, metadata object, key length) | Reject bad telemetry at boundary |
| Errors | `ApiError` enum -> HTTP status + stable machine code; internal details logged only | Predictable clients, safe logs |
| Observability | `tracing` JSON logs, request-id propagation, `/health/live`, `/health/ready` | Debuggable and deployable skeleton |
| SQL boundary | All SQL in `crates/store`; handlers call store functions | Keeps HTTP layer thin and testable |

## API reference

### Health

| Method | Path | Auth | Response |
|---|---|---|---|
| `GET` | `/health/live` | None | `200 {"status":"ok"}` |
| `GET` | `/health/ready` | None | `200` when Postgres ping OK; `503` degraded/timeout otherwise |

### Telemetry ingest

`POST /v1/telemetry` — Header: `X-Device-Key: <raw-device-key>`

```json
{
  "batch_id": "uuid",
  "readings": [
    {
      "ts": "2026-11-21T12:00:00Z",
      "key": "battery_temp",
      "value": 37.5,
      "metadata": { "unit": "C" }
    }
  ]
}
```

| Status | Meaning |
|---|---|
| `201` | Batch accepted and persisted |
| `200` | Duplicate `batch_id` for same device; no new rows inserted |
| `401` | Missing/invalid device key |
| `409` | `batch_id` already used by a different device |
| `422` | Validation failed (empty batch, bad key, non-finite value, future ts, metadata not object) |
| `503` | DB unavailable |

### Operator queries

All require `X-Operator-Key: <OPERATOR_API_KEY>`.

| Method | Path | Notes |
|---|---|---|
| `GET` | `/v1/devices` | List devices, newest first |
| `GET` | `/v1/devices/{device_id}` | 404 if unknown |
| `GET` | `/v1/devices/{device_id}/readings` | Query: `from`, `to` (RFC3339), `limit` (1..=200, default 50), `before` (`ts_rfc3339|uuid`) |
| `GET` | `/v1/devices/{device_id}/readings/latest` | Latest reading per metric key |

Readings response:

```json
{
  "items": [/* ReadingResponse */],
  "next_before": "2026-11-21T12:00:00Z|uuid" // null when no more pages
}
```

## Data model

| Table | Purpose | Key constraints/indexes |
|---|---|---|
| `devices` | Device identity and lifecycle | `status` CHECK, `last_seen` updated on ingest |
| `api_keys` | Hashed device credentials | `key_hash`, `revoked_at`; active-key lookup by hash |
| `telemetry_batches` | Ingest idempotency gate | PK `id` (client-supplied UUID), `reading_count > 0` |
| `readings` | Time-series samples | `readings(device_id, ts desc)` for range/latest queries |

Schema source: [`migrations/0001_init.sql`](migrations/0001_init.sql).

## Project structure

```text
crates/
  api/                 # HTTP layer
    src/
      auth.rs          # DeviceAuth / OperatorAuth extractors
      config.rs        # env parsing + validation
      error.rs         # ApiError -> status/code/message
      routes.rs        # router composition
      devices.rs       # operator query handlers
      telemetry.rs     # ingest handler + validation
      main.rs          # startup, migrate, serve, shutdown
    tests/             # health, telemetry, query integration tests
  domain/              # core types (Device, ApiKey, Reading, TelemetryBatch)
  store/               # DB pool, migrations, SQL queries, auth/ingest/query
migrations/            # SQL migrations
docs/                  # MVP, architecture, decisions, concepts, tech stack
scripts/               # check.sh, seed.sql, seed.ps1
```

## Local development

### Prerequisites

- Rust stable (`rust-toolchain.toml`)
- PostgreSQL 16 (local/native/WSL)
- On Windows with Smart App Control: use WSL2 for cargo test/clippy

### Configure

```bash
cp .env.example .env
```

Required env:

```env
DATABASE_URL=postgres://fleetpulse:fleetpulse@localhost:5432/fleetpulse
BIND_ADDR=0.0.0.0:8080
OPERATOR_API_KEY=dev-operator-key
RUST_LOG=fleetpulse=debug,tower_http=info,sqlx=warn
```

### Run

```bash
./scripts/check.sh          # fmt + clippy -D warnings + test
cargo run -p fleetpulse-api # applies migrations on startup
```

### Run DB-backed tests

```bash
export TEST_DATABASE_URL=postgres://fleetpulse:fleetpulse@localhost:5432/fleetpulse_test
cargo test -p fleetpulse-api -- --ignored
```

## Known gaps / tradeoffs

| Gap | Impact | Next step |
|---|---|---|
| No Postgres in this environment | Ignored DB integration tests not executed here | Run against local/CI Postgres |
| Device-key auth scans active keys | O(n) Argon2 verifies per request | Add key prefix/id lookup or cached verifier |
| No rate limiting / abuse protection | MVP only | Add tower-governor/Redis limiter before exposure |
| No metrics/benchmarks yet | Performance not quantified | Add Criterion ingest bench + Prometheus `/metrics` |
| Seed script hash placeholder | `scripts/seed.sql` needs real Argon2 hash | Regenerate from a known raw key before demo |

## Deferred (intentional)

Redis cache/rate-limit, outbox + streams, alert worker, OTA long-poll, Prometheus, Criterion, testcontainers, MQTT, JWT, OpenAPI. Reintroduction triggers are in [`docs/MVP.md`](docs/MVP.md).

## Documentation

- [`docs/MVP.md`](docs/MVP.md) — current scope and definition of done
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — target design
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — ADRs
- [`docs/CONCEPTS.md`](docs/CONCEPTS.md) — domain/system concepts
- [`docs/TECH_STACK.md`](docs/TECH_STACK.md) — dependencies and tooling