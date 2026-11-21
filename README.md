# FleetPulse

A Rust backend for connected-vehicle telemetry, built as a portfolio project for Bytebeam’s Backend Engineer (SDE 1) role. It focuses on idempotent ingestion, typed failures, and clear operational boundaries.

**Status:** M1 walking skeleton is complete. M2 telemetry ingestion is next.

## Current scope

- **Implemented (M1):** env config, Postgres pool, SQL migrations, typed `ApiError` responses, request IDs, JSON tracing, `/health/live`, `/health/ready`, graceful shutdown.
- **MVP boundary:** one Rust binary + PostgreSQL. Redis, streams/outbox, alert workers, OTA, Prometheus, and Docker are deferred. See [`docs/MVP.md`](docs/MVP.md).

## API

| Method | Path | Status | Notes |
|---|---|---|---|
| `GET` | `/health/live` | Implemented | Liveness probe; no dependency checks |
| `GET` | `/health/ready` | Implemented | Postgres check; `503` when degraded |
| `POST` | `/v1/telemetry` | Planned (M2) | Idempotent batch ingest via `batch_id` |
| `GET` | `/v1/devices` | Planned (M3) | List devices |
| `GET` | `/v1/devices/{id}/readings` | Planned (M3) | Time-range query, cursor pagination |
| `GET` | `/v1/devices/{id}/readings/latest` | Planned (M3) | Latest value per metric key |

## Quickstart

**Prerequisites:** Rust stable, PostgreSQL 16.

```bash
cp .env.example .env
# set DATABASE_URL and OPERATOR_API_KEY
./scripts/check.sh
cargo run -p fleetpulse-api
```

Verify:

```bash
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready
```

> **Windows note:** Smart App Control can block locally compiled Rust test binaries (`os error 4551`). Use WSL2 for the check loop: `export CARGO_TARGET_DIR=$HOME/target/fleetpulse && ./scripts/check.sh`.

## Project layout

```text
crates/
  api/        # axum routes, config, errors, binary
  domain/     # Device, ApiKey, TelemetryBatch, Reading
  store/      # Postgres pool, migrations, DB access
migrations/   # SQL schema
docs/         # MVP, architecture, decisions, concepts, tech stack
scripts/      # check.sh
```

## Roadmap

| Milestone | Scope | Status |
|---|---|---|
| M1 | Skeleton, migrations, health, typed errors | Done |
| M2 | Telemetry ingest, device auth, validation, `batch_id` dedupe | Next |
| M3 | Device and readings query APIs | Planned |
| M4 | Integration tests, seed script, demo walkthrough | Planned |
| M5 | Optional inline alerts | Planned |

Deferred after MVP: Redis, outbox/streams, alert worker, OTA long-poll, Prometheus, Criterion, testcontainers, MQTT, JWT, OpenAPI. Triggers are documented in [`docs/MVP.md`](docs/MVP.md).

## Documentation

- [`docs/MVP.md`](docs/MVP.md) — current scope and definition of done
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — target design
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — architecture decisions
- [`docs/CONCEPTS.md`](docs/CONCEPTS.md) — domain and system concepts
- [`docs/TECH_STACK.md`](docs/TECH_STACK.md) — dependencies and tooling