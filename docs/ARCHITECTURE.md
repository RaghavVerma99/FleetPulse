# Architecture

> **Scope note:** this document describes the target design. The current MVP is
> Postgres-only and deliberately defers Redis, streams/outbox, alerts, and OTA.
> `docs/MVP.md` is authoritative for what is being built now.
>
> **Implementation status (current):** M1/M2/M3 and M5 are implemented in code (health, ingest, device/operator auth, query APIs, inline threshold alerts — ADR-015). DB-backed integration tests are wired into CI with a Postgres service; locally they are opt-in via `scripts/check-db.sh`. The first CI run caught a test-key format regression, now fixed, and a re-run is pending. Next up: device provisioning and rate limiting. This environment has no linker, WSL, or Postgres, so CI is the verification path.

## 1. System overview

FleetPulse is a set of small Rust services (or one service with clear internal
crates) that together form a connected-device backend. The design covers core
connected-device product surfaces: telemetry, remote diagnostics, OTA
updates, alerts, and fleet-scale device management.

## 2. The big picture

```
┌──────────────┐   HTTP/MQTT    ┌───────────────────────────────┐
│   Devices    │ ────────────►  │           API (axum)          │
│ (simulated)  │  telemetry     │  ┌─────────────────────────┐  │
└──────────────┘                │  │ middleware: auth,       │  │
       ▲                        │  │ validation, tracing,    │  │
       │ long-poll              │  │ rate-limit, request-id  │  │
       │ (commands)             │  └───────────┬─────────────┘  │
       │                        └──────────────┼────────────────┘
       │                                       │
       │           ┌───────────────────────────┼───────────────────────────┐
       │           ▼                           ▼                           ▼
       │   ┌─────────────────┐        ┌─────────────────┐        ┌─────────────────┐
       │   │   PostgreSQL    │        │  Redis (cache   │        │  outbox table   │
       │   │  (device data,  │        │  + rate limit)  │        │  (in Postgres,  │
       │   │  readings,      │        │                 │        │   same tx as    │
       │   │  alerts, rules) │        └─────────────────┘        │   readings)     │
       │   └────────┬────────┘                                  └────────┬────────┘
       │            │                                                    │
       │            │                                          ┌─────────▼───────────┐
       │            │                                          │  Outbox relay       │
       │            │                                          │  → Redis Stream     │
       │            │                                          └─────────┬───────────┘
       │            │                                                    │
       │            ▼                                                    ▼
       │   ┌─────────────────┐        ┌─────────────────┐        ┌─────────────────┐
       │   │  Alert engine   │ ◄────  │  telemetry read │        │  OTA / Command  │
       │   │  (threshold /   │  eval  │  stream         │        │  worker (tokio) │
       │   │  rule evaluator)│        └─────────────────┘        └────────┬────────┘
       │   └────────┬────────┘                                           │
       │            ▼                                                    │
       │   ┌─────────────────┐                                           │
       │   │  /api/alerts    │                                           │
       │   │  + observability│                                           │
       │   └─────────────────┘                                           │
       └──────────────────────────── command queue ◄─────────────────────┘
```

The cloud never dials a device. Commands are delivered by **HTTP long-poll**:
the device holds an open `GET /v1/devices/me/commands?wait=30s`, and the worker
hands it the next queued command (ADR-011).

## 3. Request lifecycle (a telemetry upload)

1. Device sends `POST /v1/telemetry` with a JSON batch of readings and a
   client-generated `batch_id`.
2. Axum router matches the route; middleware layers run:
   - **request-id** — assign/propagate a trace id.
   - **auth** — validate the device API key, extract `device_id`.
   - **rate-limit** — check a Redis counter for this device.
   - **validation** — deserialize into a typed `TelemetryBatch`, reject bad data.
3. Handler calls the telemetry service, in **one Postgres transaction**:
   - insert the `telemetry_batches` row (`ON CONFLICT (id) DO NOTHING`);
   - if the batch already existed, commit and return `200` with the original
     reading count — the retry is a no-op;
   - insert all `readings` rows;
   - insert one `outbox` row for the batch.
4. Return `201 Created` with the accepted count.
5. After commit, best-effort update the Redis latest-reading cache. Cache writes
   are never part of the transaction; a miss falls back to Postgres.
6. The **outbox relay** polls unpublished rows, publishes them to the Redis
   Stream, and marks them published. Consumers are at-least-once and dedupe on
   `outbox.id` (ADR-010).

## 4. Offline vs online workflows

- **Online (request path)**: ingest, query, auth, cache lookups, command dispatch.
- **Background (workers)**: outbox relay, alert evaluation, OTA command delivery,
  stale-device detection.
- **Operational (human)**: replay incident, run benchmarks, inspect metrics.

## 5. Data model (core entities)

```rust
struct Device {
    id: Uuid,
    name: String,
    status: DeviceStatus,          // Provisioning | Online | Offline | Retired
    firmware_version: Option<String>,
    last_seen: Option<DateTime<Utc>>,
}

struct ApiKey {
    id: Uuid,
    device_id: Uuid,
    key_hash: String,              // argon2 hash, never the raw key
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

struct TelemetryBatch {
    id: Uuid,                      // client-supplied; the idempotency key
    device_id: Uuid,
    received_at: DateTime<Utc>,
    reading_count: i32,
}

struct Reading {
    id: Uuid,
    device_id: Uuid,
    batch_id: Uuid,
    ts: DateTime<Utc>,
    key: String,
    value: f64,
    metadata: Json<Value>,
}

struct Command {
    id: Uuid,                      // server-assigned identity
    device_id: Uuid,
    idempotency_key: String,       // client-supplied, unique per device
    kind: CommandKind,             // OtaUpdate | DiagnosticsRequest | Reboot
    payload: Json<Value>,
    status: CommandStatus,         // Queued | Delivered | Acked | Completed | Failed
    error: Option<String>,
    created_at: DateTime<Utc>,
    delivered_at: Option<DateTime<Utc>>,
    acked_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

struct AlertRule {
    id: Uuid,
    device_id: Uuid,
    metric_key: String,
    threshold: f64,
    op: Operator,                  // Gt | Gte | Lt | Lte
    for_seconds: i32,              // must hold this long before firing
    cooldown_seconds: i32,         // suppress duplicate fires
    enabled: bool,
}

struct Alert {
    id: Uuid,
    device_id: Uuid,
    rule_id: Uuid,
    severity: Severity,            // Info | Warning | Critical
    message: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

struct OutboxEvent {
    id: i64,
    topic: String,
    payload: Json<Value>,
    created_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
}
```

Indexes that matter from day one: `readings (device_id, ts DESC)`,
`telemetry_batches (id)` primary key, `commands (device_id, idempotency_key)`
unique, `outbox (id) WHERE published_at IS NULL`.

## 6. API surface

### Implemented now (MVP code)

| Method | Path | Auth | Status |
|---|---|---|---|
| GET | `/health/live` | none | Implemented |
| GET | `/health/ready` | none | Implemented (Postgres only in MVP) |
| POST | `/v1/telemetry` | device key (`X-Device-Key`) | Implemented |
| GET | `/v1/devices` | operator key (`X-Operator-Key`) | Implemented |
| GET | `/v1/devices/{id}` | operator key | Implemented |
| GET | `/v1/devices/{id}/readings` | operator key | Implemented |
| GET | `/v1/devices/{id}/readings/latest` | operator key | Implemented |
| GET | `/v1/alerts` | operator key | Implemented (fired alerts, cursor-paginated) |

Rules are stored in `alert_rules` and evaluated inline during ingest (ADR-015).
Rule creation is SQL-only in M5; `POST /v1/alerts/{id}/resolve` remains deferred.

### Target / deferred

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/metrics` | none (internal) | Prometheus scrape |
| POST | `/v1/devices/{id}/commands` | operator key | create command (`Idempotency-Key` header) |
| GET | `/v1/devices/{id}/commands` | operator key | command history |
| GET | `/v1/devices/me/commands?wait=30s` | device key | long-poll for next command |
| POST | `/v1/devices/me/commands/{id}/ack` | device key | acknowledge / complete |
| GET/POST | `/v1/alert-rules` | operator key | list / create rules |
| PATCH/DELETE | `/v1/alert-rules/{id}` | operator key | update / delete a rule |
| POST | `/v1/alerts/{id}/resolve` | operator key | close an alert |

Two credential classes in v1: **device API keys** stored hashed in Postgres, and
a single **operator API key** supplied via environment (no users table yet —
ADR-013).

## 7. Invariants and non-negotiables

- Every error is a typed `ApiError` with an HTTP status and a machine code; no
  `unwrap()` in request handlers.
- All writes to Postgres go through the `store` crate; handlers never build SQL.
- A telemetry batch is atomic: the batch row, its readings, and any alerts it
  fires commit together or not at all. The target design adds an outbox event to
  that transaction (ADR-010); it is deferred in the MVP.
- Telemetry retries are idempotent by client-supplied `batch_id`; a duplicate
  batch never fires alerts twice. Command retries (target design) use
  `(device_id, idempotency_key)`.
- Postgres is the source of truth. Redis caching and at-least-once outbox
  delivery are target-design concerns, not implemented in the MVP (ADR-014).
- Every log line carries `request_id` and `device_id` where applicable.
- Simulated data is labeled as such; never claim real fleet numbers.

## 8. Failure handling and observability

- `tracing` spans on every request; JSON logs to stdout.
- Prometheus metrics endpoint (`metrics` + `metrics-exporter-prometheus`):
  request count/latency by route, DB pool usage, cache hit ratio, outbox lag,
  queue depth, alerts fired.
- Health checks: `/health/live` (process alive), `/health/ready` (DB + Redis ok).
- Graceful shutdown on `SIGINT`/`SIGTERM`: stop accepting, drain in-flight
  requests, close pools.
- A documented "incident replay": given a saved failing payload + log excerpt,
  `scripts/replay-incident.sh` replays it against a test database and asserts
  the expected behaviour, so the regression is reproducible in one command.
