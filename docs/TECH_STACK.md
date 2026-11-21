# Tech Stack

## Core Rust
| Crate | Purpose |
|---|---|
| `tokio` | Async runtime — the engine for concurrent I/O |
| `axum` | HTTP web framework — routing, handlers, middleware |
| `serde` / `serde_json` | JSON serialization/deserialization |
| `sqlx` | Async PostgreSQL client with compile-time query checking |
| `uuid` | Stable identifiers for devices/readings/commands |
| `chrono` | Timestamps (UTC) |
| `thiserror` / `anyhow` | Typed errors and error propagation |
| `tracing` / `tracing-subscriber` | Structured logs and spans |
| `validator` | Request payload validation |
| `tower` / `tower-http` | Middleware (auth, tracing, request-id, CORS) |
| `dotenvy` | Load `.env` for local development |
| `argon2` | Hash device API keys at rest |

## Data & infra
| Component | Role |
|---|---|
| PostgreSQL 16 | Source of truth: devices, readings, alerts, rules, outbox |
| Redis 7 | Latest-reading cache, rate-limit counters, Redis Streams queue |
| Docker + Compose | Local Postgres/Redis, reproducible dev environment |

## Testing & quality
| Tool | Role |
|---|---|
| `testcontainers` | Real Postgres/Redis in integration tests (needs Docker) |
| `criterion` | Benchmarks for ingestion and query paths |
| `cargo test` | Unit + integration tests |
| `clippy` / `rustfmt` | Linting and formatting, enforced in CI |

## Observability
| Tool | Role |
|---|---|
| `tracing` + JSON subscriber | Structured logs with request ids |
| `metrics` + `metrics-exporter-prometheus` | Prometheus `/metrics` endpoint |
| `tower-http` `TraceLayer` + `RequestIdLayer` | Per-request spans and ids |
| `/health/live`, `/health/ready` | Liveness and readiness probes |

## Optional stretch
| Tool | Role |
|---|---|
| `reqwest` | Device simulator pushing telemetry over HTTP |
| MQTT (`rumqttc`) | Alternative device protocol, closer to real fleets |
| `jsonwebtoken` | JWT auth + operator roles (v2) |

## Integration notes
- `validator` does not integrate with axum out of the box: implement a small
  `ValidatedJson<T>` extractor that deserializes and returns `422` on failure.
- sqlx's compile-time macros need a live database at build time. Commit the
  `.sqlx/` query cache and set `SQLX_OFFLINE=true` in CI and for fresh clones.
- Pin exact dependency versions in `Cargo.lock` and record the resolved set in
  this file once Phase 1 wires the real dependencies.
