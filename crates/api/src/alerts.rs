use axum::extract::{Query, State};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use fleetpulse_domain::{Alert, AlertSeverity};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::OperatorAuth;
use crate::error::{from_db, ApiError};
use crate::pagination::{parse_before, validate_limit, DEFAULT_LIMIT};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/alerts", axum::routing::get(list_alerts))
}

#[derive(Debug, Deserialize)]
pub struct AlertsParams {
    pub device_id: Option<Uuid>,
    pub limit: Option<i64>,
    /// Cursor as `triggered_at_rfc3339|uuid`.
    pub before: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AlertResponse {
    pub id: Uuid,
    pub device_id: Uuid,
    pub rule_id: Uuid,
    pub severity: AlertSeverity,
    pub value: f64,
    pub message: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl From<Alert> for AlertResponse {
    fn from(alert: Alert) -> Self {
        Self {
            id: alert.id,
            device_id: alert.device_id,
            rule_id: alert.rule_id,
            severity: alert.severity,
            value: alert.value,
            message: alert.message,
            triggered_at: alert.triggered_at,
            resolved_at: alert.resolved_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AlertsResponse {
    pub items: Vec<AlertResponse>,
    pub next_before: Option<String>,
}

async fn list_alerts(
    State(state): State<AppState>,
    _auth: OperatorAuth,
    Query(params): Query<AlertsParams>,
) -> Result<Json<AlertsResponse>, ApiError> {
    let limit = validate_limit(params.limit.unwrap_or(DEFAULT_LIMIT))?;
    let (before_ts, before_id) = parse_before(params.before.as_deref())?;

    let mut alerts = fleetpulse_store::list_alerts(
        state.db.pool(),
        fleetpulse_store::AlertsQuery {
            device_id: params.device_id,
            limit: limit + 1,
            before_ts,
            before_id,
        },
    )
    .await
    .map_err(from_db)?;

    let next_before = if alerts.len() as i64 > limit {
        alerts.truncate(limit as usize);
        alerts
            .last()
            .map(|a| format!("{}|{}", a.triggered_at.to_rfc3339(), a.id))
    } else {
        None
    };

    Ok(Json(AlertsResponse {
        items: alerts.into_iter().map(Into::into).collect(),
        next_before,
    }))
}
