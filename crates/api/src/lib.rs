//! HTTP surface for FleetPulse.
//!
//! M1: health endpoints. M2: telemetry ingestion with device-key auth.

#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod devices;
pub mod error;
pub mod routes;
pub mod state;
pub mod telemetry;

use axum::Router;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Build the full application router.
pub fn app(state: AppState) -> Router {
    routes::router(state)
        // Outermost first: assign an id, trace the request, echo the id back.
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
}
