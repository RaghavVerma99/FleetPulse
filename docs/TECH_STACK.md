# Tech Stack

> MVP scope only. Deferred tools are listed at the bottom with the milestone
> that reintroduces them — see `docs/MVP.md`.

## Core Rust
| Crate | Purpose |
|---|---|
| `tokio` | Async runtime — the engine for concurrent I/O |
| `axum` | HTTP web framework — routing, handlers, middleware |
| `serde` / `serde_json` | JSON serialization/deserialization |
| `sqlx` | Async PostgreSQL client with compile-time query checking |
| `uuid` | Stable identifiers for devices/readings/batches |
| `chrono` | Timestamps (UTC) |
| `thiserror` / `anyhow` | Typed errors and error propagation |
| `tracing` / `tracing-subscriber` | Structured logs and spans |
| `validator` | Request payload validation |
| `tower` / `tower-http` | Middleware (request-id, tracing, CORS) |
| `dotenvy` | Load `.env` for local development |
| `argon2` | Hash device API keys at rest |

## Data & infra
| Component | Role |
|---|---|
| PostgreSQL 16 (native or WSL) | Source of truth: devices, API keys, batches, readings |
| `sqlx` migrations | Versioned schema in `/migrations` (ADR-012) |

## Testing & quality
| Tool | Role |
|---|---|
| `cargo test` | Unit + integration tests |
| `clippy` / `rustfmt` | Linting and formatting, enforced by `scripts/check.sh` and CI |
| Local `fleetpulse_test` database | Integration tests without Docker |

## Observability
| Tool | Role |
|---|---|
| `tracing` + JSON subscriber | Structured logs with request ids |
| `/health/live`, `/health/ready` | Liveness and readiness probes |

## Deferred (do not add yet)
| Tool | Reintroduce at |
|---|---|
| Redis (cache, rate limiting) | When reads are measured as a bottleneck |
| Redis Streams + outbox relay | When a background consumer exists (alerts) |
| `metrics` + `metrics-exporter-prometheus` | After routes stabilize (M4) |
| `testcontainers` | When CI needs containerized integration tests |
| `criterion` | After the ingest path is frozen |
| `reqwest` device simulator | After the MVP demo works end to end |
| MQTT (`rumqttc`), `jsonwebtoken` | v2 |

## Integration notes
- `validator` does not integrate with axum out of the box: implement a small
  `ValidatedJson<T>` extractor that deserializes and returns `422` on failure.
- sqlx's compile-time macros need a live database at build time. Commit the
  `.sqlx/` query cache and set `SQLX_OFFLINE=true` in CI and for fresh clones.
- Pin exact dependency versions in `Cargo.lock` and record the resolved set in
  this file once M1 wires the real dependencies.
