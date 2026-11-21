# FleetPulse

A Rust backend for connected-vehicle telemetry — the kind of system that ingests sensor data from a fleet, stores it reliably, and exposes it for operators and diagnostics.

Built as a portfolio project targeting **Bytebeam's Backend Engineer (SDE 1)** role. The emphasis is on clear boundaries, idempotent ingestion, typed failures, and operational hygiene, not feature sprawl.

> **Status: M1 walking skeleton is in.** Config, migrations, Postgres pool, typed errors, request IDs, health endpoints, and graceful shutdown are implemented. Telemetry ingest and query APIs are next.

## Why this exists

Connected-vehicle backends have a specific shape: many devices pushing small, noisy batches over unreliable networks; operators needing fast answers; and failures that must be visible, not silent. FleetPulse models that shape deliberately:

- **Idempotent by design** — a client-supplied `batch_id` makes retries safe (`201` accepted, `200` duplicate ignored).
- **Atomic writes** — a batch, its readings, and its bookkeeping commit together or not at all.
- **Typed errors** — handlers return stable machine codes and HTTP statuses; internal details are logged, never leaked.
- **Observable from day one** — request IDs, structured JSON tracing, and health probes are part of the skeleton, not a later patch.
- **Honest scope** — the MVP is one Rust binary and one PostgreSQL database. Redis, streams, alerts, and OTA are designed but deferred with explicit reintroduction triggers.

## Current capabilities (M1)

| Capability | Detail |
|---|---|
| Config | Fail-fast env parsing (`DATABASE_URL`, `BIND_ADDR`, `OPERATOR_API_KEY`, `DB_MAX_CONNECTIONS`) |
| Schema | `0001_init.sql`: `devices`, `api_keys`, `telemetry_batches`, `readings`, `(device_id, ts desc)` index |
| Persistence | `sqlx` Postgres pool, eager connect, 5s acquire timeout, workspace migrations |
| Health | `GET /health/live` always `200`; `GET /health/ready` pings Postgres with a 2s timeout, returns `503 degraded` on failure |
| Errors | `ApiError` → `{ "error": { "code", "message" } }` with stable codes |
| Ops | JSON tracing via `tracing-subscriber`, `x-request-id` propagation, graceful shutdown on Ctrl+C / SIGTERM |

## API (M1)

| Method | Path | Auth | Status | Notes |
|---|---:|---|---|---|
| GET | `/health/live` | none | implemented | Process is alive; never touches dependencies |
| GET | `/health/ready` | none | implemented | `200 {"status":"ok","checks":{"postgres":"ok"}}` or `503 degraded` |
| POST | `/v1/telemetry` | device key | planned M2 | Idempotent batch ingest |
| GET | `/v1/devices` | operator key | planned M3 | List devices |
| GET | `/v1/devices/{id}/readings` | operator key | planned M3 | Time-range, cursor-paginated |
| GET | `/v1/devices/{id}/readings/latest` | operator key | planned M3 | Latest value per metric key |

Target API surface (commands, alerts, rules, metrics) is documented in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md); [`docs/MVP.md`](docs/MVP.md) is authoritative for what is being built now.

## Architecture at a glance

```text
device/simulator
      │
      │  POST /v1/telemetry (M2)
      ▼
┌──────────────────────────────────────────┐
│ fleetpulse-api (axum)                    │
│  request-id → trace → auth → validate    │
│  typed ApiError envelope                 │
└──────────────────┬───────────────────────┘
                   │ sqlx
                   ▼
            ┌─────────────┐
            │ PostgreSQL  │  devices, api_keys,
            │ source of   │  telemetry_batches, readings
            │ truth       │
            └─────────────┘
```

Deferred target pieces (not in the binary today): Redis cache/rate-limit, transactional outbox, Redis Streams, alert worker, OTA command long-poll. See [Hold off](#hold-off).

## Repository layout

```text
.
├── crates/
│   ├── api/                 # axum app, routes, config, typed errors, main binary
│   │   ├── src/
│   │   └── tests/health.rs  # router-level health tests
│   ├── domain/              # core types: Device, ApiKey, TelemetryBatch, Reading
│   └── store/               # Postgres pool, migrations, DB health
├── migrations/
│   └── 0001_init.sql        # canonical schema (ADR-012)
├── docs/
│   ├── MVP.md               # current scope; wins over target design
│   ├── ARCHITECTURE.md      # target design
│   ├── DECISIONS.md         # ADRs
│   ├── CONCEPTS.md          # domain/background
│   └── TECH_STACK.md        # libraries and tooling
├── scripts/
│   └── check.sh             # fmt + clippy + tests (same as CI)
└── .github/workflows/ci.yml
```

## Quickstart

### Prerequisites

- Rust stable (pinned via `rust-toolchain.toml`)
- PostgreSQL 16 reachable at `DATABASE_URL`
- WSL2 recommended on Windows (see below)

### Configure

```bash
cp .env.example .env
# edit DATABASE_URL and OPERATOR_API_KEY
```

Minimum env:

```env
DATABASE_URL=postgres://fleetpulse:fleetpulse@localhost:5432/fleetpulse
BIND_ADDR=0.0.0.0:8080
OPERATOR_API_KEY=dev-operator-key
RUST_LOG=fleetpulse=debug,tower_http=info,sqlx=warn
```

### Run checks

```bash
./scripts/check.sh
```

Runs exactly what CI runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

### Run the API (requires Postgres)

```bash
cargo run -p fleetpulse-api
```

On startup the service connects eagerly, applies migrations, binds, and serves.

```bash
curl -s http://localhost:8080/health/live
# {"status":"ok"}

curl -s http://localhost:8080/health/ready
# {"status":"ok","checks":{"postgres":"ok"}}
```

When Postgres is unavailable, readiness returns `503` with `"status":"degraded"` — liveness stays `200`.

## Development notes

### Windows / Smart App Control

This machine has Windows Smart App Control enabled, which blocks unsigned locally compiled executables (`os error 4551`) — including Rust test binaries and `clippy-driver`. The supported local loop is WSL2:

```bash
wsl -d Ubuntu -u dev -- bash -lc \
  'cd /mnt/c/path/to/FleetPulse && \
   export CARGO_TARGET_DIR=$HOME/target/fleetpulse && \
   ./scripts/check.sh'
```

Keeping `CARGO_TARGET_DIR` in Linux home avoids slow `/mnt/c` build IO.

### Postgres without Docker

Docker is intentionally removed from the MVP dev path. Pick one:

1. **Native Windows install** — `winget install PostgreSQL.PostgreSQL.16`; create `fleetpulse` and `fleetpulse_test` databases. Simple service, no container networking.
2. **Postgres in WSL2** — install via apt and run locally inside Ubuntu.
3. **Hosted free tier** — Neon/Supabase for zero local install; acceptable for early dev, adds network dependency to tests.

## Roadmap

| Milestone | Focus | State |
|---|---|---|
| M1 | Walking skeleton: config, migration, pool, typed errors, health, shutdown | **done** (DB-backed ready test pending live Postgres) |
| M2 | Ingest: `POST /v1/telemetry`, device API-key auth, validation, atomic insert, `batch_id` dedupe | next |
| M3 | Query: devices, readings range with cursor pagination, latest-per-metric | planned |
| M4 | Quality: integration tests, seed script, README with real responses | planned |
| M5 | Optional: inline threshold alerts | planned |

## Hold off

These are good ideas, deliberately not in the MVP. Each has a trigger.

| Deferred | Why now | Bring back when |
|---|---|---|
| Redis cache / rate limit | Extra service, marginal at MVP scale | Read latency or abuse becomes measured |
| Outbox + Redis Streams | No background consumer yet | Alert worker or async consumer exists |
| Alert worker | Streaming subsystem; M5 can be inline first | Inline eval is insufficient |
| OTA commands / long-poll | Second vertical slice | Ingest + query are demoable |
| Prometheus metrics | Route shapes still moving | Routes stabilize (M4) |
| Criterion benches | Premature on a moving path | Ingest path is frozen |
| Testcontainers | Docker removed from local path | CI/containerized integration needed |
| MQTT, JWT/roles, rollups, multi-tenancy, OpenAPI | Scope creep for SDE1 portfolio | After MVP demo |

## Design docs

- [`docs/MVP.md`](docs/MVP.md) — current scope and definition of done
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — target system design
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — ADRs, including outbox, long-poll, and MVP cut
- [`docs/CONCEPTS.md`](docs/CONCEPTS.md) — telemetry, idempotency, outbox, command delivery
- [`docs/TECH_STACK.md`](docs/TECH_STACK.md) — selected crates and deferred tooling

## License

Not yet specified. Add a license before publishing publicly.