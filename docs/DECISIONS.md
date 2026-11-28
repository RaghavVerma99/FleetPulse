# Architecture Decision Records (ADRs)

Format: **Context → Decision → Alternatives → Consequences**.

## ADR-001: Rust with axum + tokio
**Context:** JD requires Rust backend APIs. **Decision:** axum (async web
framework) on tokio (async runtime). **Alternatives:** actix-web (heavier,
steeper), rocket (less mature async). **Consequences:** type-safe handlers,
excellent ecosystem, good performance; team must learn async Rust idioms.

## ADR-002: PostgreSQL + sqlx for persistence
**Context:** Need reliable relational storage for devices, readings, alerts.
**Decision:** Postgres with sqlx (compile-time checked async SQL).
**Alternatives:** Diesel (ORM, compile-time but less flexible), DynamoDB (no
SQL). **Consequences:** SQL matches JD; migrations managed with `sqlx migrate`.
**Current deviation:** the MVP uses `sqlx::query*()` runtime functions rather
than `query*!` macros, so queries are checked by the DB-backed tests, not at
compile time. This keeps fresh clones buildable without Postgres; converting to
macros plus a committed `.sqlx/` cache remains a future hardening task.

## ADR-003: Redis for cache + streams
**Status: deferred (ADR-014) — target design, not implemented.**
**Context:** Good-to-have in JD: caching, queues. **Decision:** Redis for latest
reading cache, rate-limit counters, and Redis Streams as the event queue.
**Alternatives:** in-memory cache (lost on restart), RabbitMQ/Kafka (overkill).
**Consequences:** fast and simple; single Redis dependency; cache invalidation
must be handled.

## ADR-004: Crate-based modular monolith
**Context:** SDE1 scope — avoid microservices complexity. **Decision:** one
binary with internal crates: api, domain, store, telemetry, alerts.
**Alternatives:** separate microservices (ops overhead), single crate (muddled
boundaries). **Consequences:** clear module boundaries, easy to test, can split
later.

## ADR-005: Typed errors everywhere
**Context:** JD asks for clear failure handling. **Decision:** `thiserror`-based
`ApiError` with HTTP status + error code; no `unwrap` in handlers.
**Alternatives:** ad-hoc strings, panics. **Consequences:** predictable API
responses; easier debugging from logs.

## ADR-006: Observability from day one
**Status: partially implemented — tracing JSON logs, request ids, and both
health probes are live; Prometheus `/metrics` and incident replay are deferred.**
**Context:** JD requires observability and reproducing production issues from
logs/telemetry. **Decision:** `tracing` structured logs, Prometheus `/metrics`,
request ids, and an incident-replay script. **Alternatives:** add later.
**Consequences:** demonstrates operational maturity; small upfront cost.

## ADR-007: Fixtures + testcontainers integration tests
**Status: superseded for the MVP — real Postgres runs in CI with `#[ignore]`
tests; testcontainers is deferred (no Docker in the target environment).**
**Context:** Need automated testing and realistic DB tests. **Decision:** SQL
fixtures + `testcontainers` to spin real Postgres/Redis in tests.
**Alternatives:** mocks only (miss SQL bugs). **Consequences:** tests are slower
but catch real integration issues; requires a working Docker daemon locally.

## ADR-008: Benchmarks with Criterion
**Status: deferred (ADR-014) — not implemented.**
**Context:** JD mentions benchmarks. **Decision:** Criterion benches for the
telemetry ingestion hot path and query path. **Alternatives:** no benchmarks.
**Consequences:** measured performance numbers for the README; shows rigor.

## ADR-009: Simulated devices, honest labeling
**Status: deferred — the seed device is simulated; the simulator script is not
built yet.**
**Context:** No real fleet. **Decision:** a small device simulator that emits
telemetry; all data explicitly labeled simulated. **Alternatives:** fake real
fleet claims. **Consequences:** safe and honest; still demonstrates the system.

## ADR-010: Transactional outbox for event publishing
**Status: deferred (ADR-014) — target design; no outbox table or relay exists.**
**Context:** Telemetry must be written to Postgres *and* published to a Redis
Stream. Writing to both is a dual write: a crash between them either loses an
event or produces one for data that never committed. **Decision:** in the same
transaction as the readings, insert an `outbox` row. A relay worker publishes
unpublished rows and marks them sent. **Alternatives:** publish after commit and
accept loss; two-phase commit (overkill); CDC via Debezium (too much infra).
**Consequences:** delivery is at-least-once, so consumers must dedupe on
`outbox.id`; adds one worker and a poll query, both cheap. This is the standard
reliable-messaging pattern and is worth being able to explain.

## ADR-011: HTTP long-poll for command delivery
**Status: deferred (ADR-014) — target design; no command endpoints exist.**
**Context:** Simulated devices sit behind NAT and cannot accept inbound
connections, so the cloud cannot push to them. **Decision:** devices hold
`GET /v1/devices/me/commands?wait=30s` open; the server returns a queued command
as soon as one exists or `204` when the wait expires. **Alternatives:** MQTT
(realistic but adds a broker and auth story), WebSocket (more reconnection
state), short polling (wasteful). **Consequences:** no extra infrastructure,
easy to test and demo; command latency is bounded by the poll timeout. MQTT can
replace this later behind the same queue without touching the command schema.

## ADR-012: Migrations live at the workspace root
**Context:** sqlx defaults to a single `migrations/` directory, but the `store`
crate owns persistence. **Decision:** SQL files live in `/migrations`; the
`store` crate owns the runner (`sqlx::migrate!`). **Alternatives:** migrations
inside `crates/store` (works, but diverges from sqlx defaults and CI tooling).
**Consequences:** one canonical location; `sqlx migrate` and `sqlx prepare` work
without extra flags.

## ADR-013: Device API keys + static operator key in v1
**Context:** Telemetry needs per-device identity; operator endpoints need some
protection, but a full user/role system is out of scope for SDE1.
**Decision:** devices authenticate with a hashed API key stored in Postgres;
operators use one API key from the environment. **Alternatives:** JWT + users
table (more surface, no extra signal), no auth (not defensible).
**Consequences:** device scoping is enforced properly; operator auth is coarse
and explicitly marked as a v2 item (JWT + roles).

## ADR-014: MVP is Postgres-only; Redis and Docker are deferred
**Context:** The target design needs Redis for cache, rate limiting, and
streams, plus Docker for local services. Both add operational surface before a
single endpoint exists. **Decision:** build the MVP against one PostgreSQL
database; defer Redis, the outbox relay, the alert worker, and Docker until the
MVP demo is complete. **Alternatives:** build the full architecture up front
(slow, unrewarding), swap Postgres for SQLite (loses the JD-relevant SQL and
concurrency story). **Consequences:** fewer moving parts and a faster path to a
demo; the deferred pieces are documented with reintroduction triggers in
`docs/MVP.md`, so this is a deliberate cut rather than an omission.

