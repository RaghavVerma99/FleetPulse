# FleetPulse

> A Rust backend for connected-vehicle telemetry, fleet device management, OTA
> commands, and alerting — built end to end as a portfolio project targeting
> Bytebeam's Backend Engineer (SDE 1) role.

FleetPulse is the software side of a connected-vehicle platform. Vehicles (or
simulated devices) stream telemetry to the cloud; the cloud validates it,
stores it, exposes query APIs, issues over-the-air (OTA) commands, evaluates
alert rules, and lets operators debug production issues from logs and metrics.

Read `docs/ARCHITECTURE.md` for the full design, `docs/DECISIONS.md` for why
each choice was made, `docs/CONCEPTS.md` for the theory behind every component,
and `docs/TECH_STACK.md` for the concrete libraries.

## Status

**Phase 0 complete: repository, toolchain, and CI scaffold.** No service code yet.

Done:
- Cargo workspace (`api`, `domain`, `store`), pinned stable toolchain.
- Local build/test/clippy loop verified on Linux (WSL2 Ubuntu, Rust 1.100):
  `fmt`, `clippy -D warnings`, and `cargo test` all pass.
- Docker Compose for Postgres 16 and Redis 7 with healthchecks, committed but
  not yet started on this machine.
- `.env.example`, `.gitignore`, `.gitattributes`, `scripts/check.sh`.
- GitHub Actions workflow mirroring `scripts/check.sh`.
- Architecture corrections: API surface, transactional outbox, long-poll
  command delivery, idempotency keys, alert durations, migrations location.

Next: Phase 1, the walking skeleton — domain types, first migration, config,
typed errors, and `/health/live` + `/health/ready`.

## Quickstart (development environment)

Prerequisites: Rust (stable, via rustup). Docker Desktop is needed from Phase 2
onward for Postgres/Redis and testcontainers.

```bash
cp .env.example .env          # adjust if your ports differ
cargo build                   # compiles the workspace
./scripts/check.sh            # fmt + clippy + tests (same as CI)

docker compose up -d          # Postgres + Redis (Phase 2 onward)
docker compose ps             # both should report healthy
```

The service itself starts in Phase 1 (`cargo run -p fleetpulse-api`).

### Windows notes

Windows **Smart App Control** blocks unsigned binaries, including every locally
compiled Rust test executable and `clippy-driver.exe` (`os error 4551`). The
supported workaround used here is to build and test inside WSL2:

```bash
wsl -d Ubuntu -u dev -- bash -lc \
  'cd /mnt/c/path/to/FleetPulse && \
   export CARGO_TARGET_DIR=$HOME/target/fleetpulse && \
   ./scripts/check.sh'
```

Keeping `CARGO_TARGET_DIR` in the Linux home avoids the slow `/mnt/c` filesystem.
- If you prefer native Windows builds, install **Visual Studio Build Tools** with
  the *Desktop development with C++* workload, and turn Smart App Control off
  (one-way, cannot be re-enabled without reinstalling Windows).
- Docker Desktop requires WSL2 on Windows 11; enable it before
  `docker compose up -d`.

## Layout

```
fleetpulse/
├── crates/
│   ├── api/          # axum routes, middleware, DTOs
│   ├── domain/       # Device, Reading, Command, Alert types
│   └── store/        # sqlx repos + migration runner
├── migrations/       # SQL schema (single canonical location)
├── fixtures/         # seed data
├── benches/          # criterion benches
├── tests/            # integration tests (testcontainers)
├── docs/             # architecture, decisions, concepts, tech stack
├── scripts/          # replay-incident.sh, seed.sh
├── .github/workflows # CI: fmt, clippy, test
└── Dockerfile, docker-compose.yml
```

Directories marked above but not yet present are created by the phase that
needs them — an empty `fixtures/` folder communicates nothing.

## Roadmap

| Phase | Deliverable |
|---|---|
| 0 | Toolchain, workspace, compose, CI, corrected design docs |
| 1 | Walking skeleton: domain types, migration, config, health checks |
| 2 | Ingestion slice: auth, validation, atomic batch insert, dedupe, outbox, bench |
| 3 | Query APIs: devices, readings range + cursor pagination, cache-through latest |
| 4 | Commands/OTA: idempotent create, long-poll delivery, ack lifecycle, simulator |
| 5 | Alerts: rule CRUD, consumer-group worker, sustained duration, cooldown |
| 6 | Observability: metrics, request ids, seed + load scripts, incident replay |
| 7 | Hardening: testcontainers suite, OpenAPI, Dockerfile, measured numbers |

Out of scope for v1, documented as future work: MQTT transport, JWT/operator
roles, telemetry rollups, multi-tenancy.
