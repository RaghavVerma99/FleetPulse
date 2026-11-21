# Concepts Behind FleetPulse

> **Scope:** this explains the target design, including components the MVP
> defers (Redis, streams, outbox relay, OTA). `docs/MVP.md` defines what is
> being built now.

This explains the *why* of every component in plain terms, then the deeper
mechanics. Read this to understand the system, not just its parts.

---

## 1. Connected vehicles and telemetry (the domain)

A connected vehicle has sensors and a cellular connection. It periodically
sends **telemetry** — measurements like speed, battery temperature, GPS, fault
codes — to the cloud. The cloud must:
- ingest huge numbers of small messages efficiently,
- store them for later analysis,
- let operators query "what was device X's temperature last hour?",
- push commands back to the device (OTA updates, diagnostics requests),
- raise alerts when something is wrong.

That is exactly Bytebeam's product surface, and what FleetPulse simulates.

## 2. HTTP APIs (how clients talk to the system)

An API is a contract: `POST /v1/telemetry` means "here is a batch of readings".
**REST** conventions: POST creates data, GET reads, PUT/PATCH updates, DELETE
removes. Status codes: `200` ok, `201` created, `400` bad input, `401`
unauthorized, `500` server error. Axum makes writing these handlers type-safe.

## 3. Async programming (tokio)

A backend serves many requests at once. Naive threads are expensive. **Async**
lets one thread handle many I/O operations by pausing a request while it waits
for the database or network, then resuming. `tokio` is Rust's async runtime;
`async fn` handlers run on it. This is why the JD lists async as good-to-have.

## 4. Web framework (axum)

Axum turns HTTP requests into Rust function calls. You define routes
(`Router::new().route("/v1/telemetry", post(handler))`) and handlers are plain
async functions that extract typed data from the request (JSON body, path
params, headers) and return typed responses. Middleware layers wrap every
request for cross-cutting concerns.

## 5. Data access and SQL (PostgreSQL + sqlx)

PostgreSQL stores structured data reliably with ACID transactions. SQL is the
language for querying it. `sqlx` lets you write SQL that is checked against the
real database schema **at compile time**, catching typos before running.
Migrations are versioned SQL files that evolve the schema safely.

## 6. Validation

Never trust client input. A JSON payload might be missing fields, have wrong
types, or contain impossible values (negative temperature). Validation rejects
bad data at the boundary so the rest of the system handles only valid data. The
`validator` crate derives validation rules on structs.

## 7. Authorization

Telemetry from device A must not be readable as device B. **Auth** answers "who
are you?" (API key/JWT); **authorization** answers "are you allowed?".
Middleware extracts credentials and attaches the authenticated `device_id` or
operator role to the request context; handlers use it to scope queries.

## 8. Caching (Redis)

Reading the latest value for a device is frequent. Hitting Postgres every time
is slow. A cache stores hot data in memory (Redis) for fast access. Tradeoff:
the cache can be stale — so we set a TTL and invalidate on new writes. Cache hit
ratio is a key metric.

## 9. Queues / streams (Redis Streams)

Some work shouldn't block the request: evaluating alerts, delivering OTA
commands. The API pushes an event onto a **queue**; a background worker consumes
it. This decouples ingestion from processing, smooths spikes, and allows retries.
Redis Streams is a lightweight append-only log with consumer groups.

## 10. Observability (logs, metrics, traces)

- **Logs**: structured events (`tracing`) with request id — searchable.
- **Metrics**: numeric time series (Prometheus) — request latency, error rate,
  cache hits, queue depth — for dashboards and alerts.
- **Traces**: a request's path across components, tied by a trace id.

Together they let you answer "why was this request slow?" or "reproduce that
production failure" — directly matching the JD's incident requirement.

## 11. Failure handling (typed errors)

In Rust, errors are values. `Result<T, E>` forces you to handle failure. A typed
`ApiError` enum (via `thiserror`) maps domain errors to HTTP responses: a missing
device becomes `404`, invalid input `400`, DB failure `500`. No `unwrap()` in
handlers means no surprise panics taking down the service.

## 12. Testing (unit, integration, fixtures)

- **Unit tests** test small functions in isolation.
- **Integration tests** spin up real Postgres/Redis with `testcontainers` and
  exercise the API end to end — catching SQL and wiring bugs.
- **Fixtures** are seed data (SQL or Rust builders) that put the DB in a known
  state for tests.

## 13. Benchmarks (Criterion)

A benchmark measures how fast code is under controlled conditions. Criterion
gives statistically reliable numbers and detects regressions. Benchmarking the
telemetry ingestion path shows you understand performance, not just correctness.

## 14. OTA updates and command lifecycle

Over-the-air updates must be reliable: a command is created, queued, delivered
to the device, acknowledged, and marked complete. Making commands **idempotent**
(keyed by `command_id`) means a retried request doesn't flash the device twice —
a critical real-world safety property.

## 15. Alerting

An **alert rule** says "if temperature > 80°C for 5 minutes, fire a critical
alert". The alert engine consumes telemetry events, evaluates rules, and writes
alerts. This is a stream-processing problem: evaluate each reading as it arrives
rather than scanning the whole database.

## 16. Why a modular monolith (crates)

Instead of many deployed microservices (hard to operate), FleetPulse is one
binary split into internal crates with clear responsibilities. Each crate has a
single job and can be tested independently. If the system grows, a crate can be
extracted into its own service later. This is the right complexity level for an
SDE 1 project.

## 17. Transactional outbox (reliable events)

Writing to two systems is a trap. If the handler commits readings to Postgres
and then pushes to a queue, a crash in between loses the event; if it pushes
first, a failed commit leaves an event for data that never existed. There is no
way to make two independent systems commit atomically.

The **outbox pattern** sidesteps this: the event is written as a row in the
*same* Postgres transaction as the data. A separate relay worker reads
unpublished rows, publishes them, and marks them sent. The database transaction
is the single point of truth, so the event exists exactly when the data exists.

The cost is that the relay can publish twice (crash after publishing, before
marking), so delivery is **at-least-once** and consumers must be idempotent —
dedupe on the outbox row id. This is the honest, standard tradeoff, and being
able to explain *why* exactly-once is not achievable here is more valuable than
claiming it.

## 18. Device command delivery (long-poll)

The cloud cannot open a connection to a vehicle behind a cellular NAT. So the
device pulls: it holds a request open (`GET .../commands?wait=30s`) and the
server replies the moment a command is queued, or with `204` when the timeout
expires. This gives near-push latency with plain HTTP and no broker.

The command itself moves through explicit states — queued, delivered,
acknowledged, completed or failed — and carries a client idempotency key so a
retried create does not flash the same firmware twice.

## 19. Bringing it all together

A device sends a reading → axum handler validates and authorizes → sqlx writes
to Postgres → Redis cache updated → event pushed to stream → alert worker
evaluates rules → alert stored → metrics/labels let you debug. Every JD bullet
maps to one step in this flow. That coherence — not just a list of features —
is what makes the project convincing.
