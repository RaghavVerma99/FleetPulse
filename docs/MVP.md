# MVP Scope

The goal is a **demoable product**, not the full target architecture. One Rust
binary, one PostgreSQL database, three vertical slices. Everything else is
explicitly deferred.

> Target design: `ARCHITECTURE.md`. This document wins where they disagree.

## Current implementation status

- **M1/M2/M3 implemented in code**: health, config, migrations, telemetry ingest, device/operator auth, device/readings query APIs.
- **Verification status**: `scripts/check.sh` runs fmt, clippy `-D warnings`, and DB-free tests. CI additionally runs `cargo test --workspace -- --ignored` against a Postgres service; locally use `TEST_DATABASE_URL=... bash scripts/check-db.sh`.
- **This environment**: no MSVC linker, WSL, or Postgres is available, so compilation and DB-backed tests could not be run locally. CI is the verification path.

## The demo

```text
1. Seed a device with an API key.
2. POST /v1/telemetry with a batch of readings   -> 201, accepted: N
3. Replay the same batch_id                      -> 200, duplicate ignored
4. GET /v1/devices                               -> device listed, last_seen updated
5. GET /v1/devices/{id}/readings?from=...&to=... -> paginated readings
6. GET /v1/devices/{id}/readings/latest          -> latest value per metric
7. Stop Postgres                                 -> /health/ready returns 503
```

That is the whole MVP. It proves ingestion, idempotency, persistence, querying,
auth, and operational health — the core of the JD.

## In scope

| Milestone | Deliverable | State |
|---|---|---|
| M1 Skeleton | Config, Postgres pool, `0001_init.sql`, tracing, typed `ApiError`, health endpoints, graceful shutdown | Done |
| M2 Ingest | `POST /v1/telemetry`, device API-key auth, validation, one transaction, `batch_id` dedupe, `201`/`200`/`409` semantics | Done |
| M3 Query | Device list/detail, readings time-range with cursor pagination, latest-per-metric | Done |
| M4 Quality | DB-backed test run in CI/local Postgres, seed script with a verified hash, README demo commands, typed error envelope | In progress (CI found a test-key format bug; fix committed, awaiting green re-run) |
| M5 Alerts | Inline threshold alerts on ingest + `GET /v1/alerts` | **Next** |

Schema in M1: `devices`, `api_keys`, `telemetry_batches`, `readings`.
Index: `readings (device_id, ts DESC)`.
M4 adds `0002_indexes.sql` for latest-per-metric, cascade, and cursor queries.

## Next: M5 inline alerts

Scope for the next slice (see ADR-015):

1. Migration `0003_alerts.sql`: `alert_rules` and `alerts` tables plus indexes
   for `(device_id, triggered_at DESC)` and enabled rules per device.
2. Ingest-time evaluation inside the existing batch transaction: for each
   enabled rule matching a metric key in the batch, compare the newest value and
   insert an alert when the threshold is crossed and the cooldown has elapsed.
3. Operator endpoint `GET /v1/alerts` with `device_id`, `limit`, and `before`
   cursor; response uses `{items, next_before}` like the other list endpoints.
4. Tests: rule fires, cooldown suppresses a second fire, non-matching metric does
   not fire, and the query is filtered/paginated correctly.

Non-goals for M5: background worker, Redis/outbox, rule CRUD endpoints, and a
"condition must hold for N seconds" window. Add those only if inline evaluation
proves insufficient (see the reintroduction trigger in the table below).

Definition of done: the seven-step demo still passes, the new alert demo step
is documented with real responses, and `cargo test --workspace -- --ignored` is
green in CI.

## Hold off (do not build yet)

| Deferred | Why now | Reintroduce when |
|---|---|---|
| Redis (cache, rate limit) | A second service for marginal MVP value; Postgres indexed reads are fast enough at this scale | Ingest or read latency is measured as a problem |
| Redis Streams + outbox relay | Only needed once a background consumer exists; the atomic insert already prevents partial batches | Alerts or any async consumer is added |
| Alert engine worker | Stream processing is a second system; inline evaluation covers the demo | M5 proves insufficient |
| OTA commands + long-poll | A second vertical slice; dilutes focus until telemetry/query is solid | After M3 is demoable |
| Prometheus metrics | Endpoints and route names will still change | Routes stabilize (M4) |
| Criterion benches | Measuring a path that is still being reshaped wastes effort | Ingest path frozen |
| Testcontainers | Requires Docker, which is removed from this machine | CI needs containerized integration tests |
| Incident replay, load generator, simulator | Demo polish, not product behaviour | MVP is demoable end to end |
| MQTT, JWT/roles, rollups, multi-tenancy, OpenAPI | Scope creep with no extra signal for SDE1 | After the MVP ships |
| Docker / Compose | Native Postgres is enough for one developer | Deployment or CI parity needs it |

## Postgres without Docker

Pick one, in order of preference:

1. **Native Windows install** — `winget install PostgreSQL.PostgreSQL.16`, then
   create the `fleetpulse` user/database. Runs as a Windows service; no WSL
   networking to debug.
2. **Postgres inside WSL2** — `apt-get install postgresql`, start with
   `pg_ctlcluster 16 main start`, connect from Windows over `localhost`.
3. **Hosted free tier** (Neon/Supabase) — zero install, but adds a network
   dependency to every test run.

Tests use a separate database (`fleetpulse_test`) created by the test harness.

## Definition of done

- `scripts/check.sh` passes (fmt, clippy, tests).
- The seven-step demo runs from a clean database with one seed command.
- README shows the actual commands and responses, no invented numbers.
- No `unwrap()` in request handlers; all errors are typed.
