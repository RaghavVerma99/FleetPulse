# MVP Scope

The goal is a **demoable product**, not the full target architecture. One Rust
binary, one PostgreSQL database, three vertical slices. Everything else is
explicitly deferred.

> Target design: `ARCHITECTURE.md`. This document wins where they disagree.

## The demo

```
1. Seed a device with an API key.
2. POST /v1/telemetry with a batch of readings   -> 201, accepted: N
3. Replay the same batch_id                      -> 200, duplicate ignored
4. GET /v1/devices                               -> the device, last seen updated
5. GET /v1/devices/{id}/readings?from=...&to=...  -> paginated readings
6. GET /v1/devices/{id}/readings/latest          -> latest value per metric
7. Stop Postgres                                  -> /health/ready returns 503
```

That is the whole MVP. It proves ingestion, idempotency, persistence, querying,
auth, and operational health — the core of the JD.

## In scope

| Milestone | Deliverable |
|---|---|
| M1 Skeleton | Config, Postgres pool, `0001_init.sql`, tracing, typed `ApiError`, `/health/live`, `/health/ready`, graceful shutdown |
| M2 Ingest | `POST /v1/telemetry`, device API-key auth, validation, one transaction, `batch_id` dedupe, `201`/`200` semantics |
| M3 Query | Device list/detail, readings time-range with cursor pagination, latest-per-metric |
| M4 Quality | Integration tests against a local test database, seed script, README with real curl examples |
| M5 Optional | Inline threshold alerts on ingest + `GET /v1/alerts` |

Schema in M1: `devices`, `api_keys`, `telemetry_batches`, `readings`.
Index: `readings (device_id, ts DESC)`.

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
