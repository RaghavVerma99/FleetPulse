# FleetPulse

FleetPulse is a Rust backend for connected-vehicle telemetry. It ingests device batches, persists readings in PostgreSQL, and exposes operator query APIs with idempotent writes, typed errors, and operational health checks.

Built as a portfolio project demonstrating backend engineering for connected-device telemetry systems.

## Status

| Milestone | State | Notes |
|---|---|---|
| M1 Skeleton | Complete | Config, migrations, DB pool, tracing, health, graceful shutdown |
| M2 Telemetry ingest | Complete | Device auth, validation, atomic batch insert, `batch_id` dedupe |
| M3 Query APIs | Complete | Device list/detail, readings range + cursor, latest-per-metric |
| M4 Quality | In progress | CI now runs the DB-backed suite; domain types wired in; sqlx docs corrected. Awaiting a green CI run. |
| M5 Alerts | Optional | Inline threshold evaluation |

> `scripts/check.sh` runs fmt, clippy `-D warnings`, and DB-free tests. The
> `#[ignore]` DB-backed tests run in CI against a Postgres service, or locally
> with `TEST_DATABASE_URL=... bash scripts/check-db.sh`. They use `serial_test`
> because they share a database.

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
| Auth | `X-Device-Key` uses `fp_<key_id>_<secret>` format for direct `api_keys` PK lookup + Argon2id verify; unprefixed keys are rejected; `X-Operator-Key` static env key compared in constant time | Avoids O(n) Argon2 verifies on the hot path while keeping hash-at-rest (ADR-013) |
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
| `GET` | `/v1/devices` | List devices, newest first; `limit` (1..=200, default 50), `before` cursor. Returns `{items, next_before}` |
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
| `api_keys` | Hashed device credentials | `key_hash`, `revoked_at`; prefixed key embeds `id` for direct lookup; hash verified in Rust |
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
      extract.rs       # ValidatedJson<T> extractor
      routes.rs        # router composition
      devices.rs       # operator query handlers
      telemetry.rs     # ingest handler + boundary validation
      main.rs          # startup, migrate, serve, shutdown
    tests/             # health, telemetry, query integration tests
  domain/              # shared types used by store and api (Device, ApiKey, Reading, ...)
  store/               # DB pool, migrations, SQL queries, auth/ingest/query
migrations/            # SQL migrations (0001 schema, 0002 indexes)
docs/                  # MVP, architecture, decisions, concepts, tech stack
scripts/               # check.sh, check-db.sh, seed.sql, seed.ps1
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
bash scripts/check-db.sh          # cargo test --workspace -- --ignored
```

DB-backed integration tests share a database and are marked `#[serial]` to avoid pool exhaustion/flakes. CI runs them automatically.

### Demo

Start the API, then apply the seed (`powershell -File scripts/seed.ps1` on
Windows, or `psql "$DATABASE_URL" -f scripts/seed.sql` elsewhere). The demo key
is printed in `scripts/seed.sql` and is dev-only.

```bash
# 1. ingest -> 201 {"status":"accepted",...,"accepted":2}
curl -i -X POST localhost:8080/v1/telemetry \
  -H 'content-type: application/json' \
  -H 'x-device-key: fp_00000000000000000000000000000101_3372f3f3d3804f3191361e0b5ad643fb' \
  -d '{"batch_id":"11111111-1111-4111-8111-111111111111","readings":[{"ts":"2026-11-28T12:00:00Z","key":"battery_temp","value":37.5,"metadata":{"unit":"C"}},{"ts":"2026-11-28T12:00:01Z","key":"speed_kph","value":52.0}]}'

# 2. replay the same batch_id -> 200 {"status":"duplicate",...,"accepted":2}
# 3. operator queries -> 200
curl -i -H 'x-operator-key: dev-operator-key' localhost:8080/v1/devices
curl -i -H 'x-operator-key: dev-operator-key' localhost:8080/v1/devices/00000000-0000-0000-0000-000000000001/readings?limit=10
curl -i -H 'x-operator-key: dev-operator-key' localhost:8080/v1/devices/00000000-0000-0000-0000-000000000001/readings/latest
```

The ingest/replay assertions are automated by `seed_key_authenticates_and_batch_is_idempotent`
in `crates/api/tests/telemetry.rs`.

## Known gaps / tradeoffs

| Gap | Impact | Next step |
|---|---|---|
| Argon2 verification is CPU-bound | Correct for security, but auth cost is non-trivial under load | Add rate limiting; consider key-cache only with careful rotation/revocation semantics |
| No rate limiting / abuse protection | MVP only | Add tower-governor/Redis limiter before exposure |
| DB-backed tests share one Postgres database | Serialized with `serial_test`; CI supplies a Postgres service, local runs need `TEST_DATABASE_URL` | Move to testcontainers if isolation is needed |
| sqlx queries are runtime-checked, not compile-time checked | Query typos surface in DB tests, not at build time (ADR-002 deviation) | Convert to `query*!` macros and commit `.sqlx/` |
| No metrics/benchmarks yet | Performance not quantified | Add Criterion ingest bench + Prometheus `/metrics` |
| Seed contains a demo key/hash | Fine for local demo; unsafe to reuse outside dev. Authentication is now covered by an integration test | Generate per-environment keys; never commit real secrets |

## Deferred (intentional)

Redis cache/rate-limit, outbox + streams, alert worker, OTA long-poll, Prometheus, Criterion, testcontainers, MQTT, JWT, OpenAPI. Reintroduction triggers are in [`docs/MVP.md`](docs/MVP.md).

## Documentation

- [`docs/MVP.md`](docs/MVP.md) — current scope and definition of done
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — target design
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — ADRs
- [`docs/CONCEPTS.md`](docs/CONCEPTS.md) — domain/system concepts
- [`docs/TECH_STACK.md`](docs/TECH_STACK.md) — dependencies and tooling