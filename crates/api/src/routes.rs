use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::alerts;
use crate::devices;
use crate::state::AppState;
use crate::telemetry;

const READY_TIMEOUT: Duration = Duration::from_secs(2);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .merge(telemetry::router())
        .merge(devices::router())
        .merge(alerts::router())
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct LiveBody {
    status: &'static str,
}

/// Liveness: the process is up and serving. Never touches dependencies, so a
/// database outage does not cause a restart loop.
async fn live() -> impl IntoResponse {
    (StatusCode::OK, Json(LiveBody { status: "ok" }))
}

#[derive(Debug, Serialize)]
struct ReadyBody {
    status: &'static str,
    checks: ReadyChecks,
}

#[derive(Debug, Serialize)]
struct ReadyChecks {
    postgres: &'static str,
}

/// Readiness: can this instance serve traffic right now?
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let postgres = match tokio::time::timeout(READY_TIMEOUT, state.db.ping()).await {
        Ok(Ok(())) => "ok",
        Ok(Err(error)) => {
            tracing::warn!(%error, "postgres readiness check failed");
            "unavailable"
        }
        Err(_) => {
            tracing::warn!("postgres readiness check timed out");
            "timeout"
        }
    };

    if postgres == "ok" {
        (
            StatusCode::OK,
            Json(ReadyBody {
                status: "ok",
                checks: ReadyChecks { postgres },
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyBody {
                status: "degraded",
                checks: ReadyChecks { postgres },
            }),
        )
    }
}
