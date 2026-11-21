use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

use crate::auth::DeviceAuth;
use crate::error::ApiError;
use crate::state::AppState;

const MAX_READINGS_PER_BATCH: usize = 1000;
const MAX_METRIC_KEY_LEN: usize = 128;
const MAX_FUTURE_SKEW: Duration = Duration::minutes(5);

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/telemetry", axum::routing::post(ingest_telemetry))
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct TelemetryBatchRequest {
    pub batch_id: Uuid,
    #[validate(length(min = 1, max = 1000, message = "must contain 1..=1000 readings"))]
    #[validate(nested)]
    pub readings: Vec<ReadingRequest>,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct ReadingRequest {
    pub ts: DateTime<Utc>,
    #[validate(length(min = 1, max = 128, message = "must be 1..=128 chars"))]
    pub key: String,
    pub value: f64,
    #[serde(default = "default_metadata")]
    pub metadata: Value,
}

fn default_metadata() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub status: &'static str,
    pub batch_id: Uuid,
    pub accepted: usize,
}

async fn ingest_telemetry(
    State(state): State<AppState>,
    auth: DeviceAuth,
    Json(payload): Json<TelemetryBatchRequest>,
) -> Result<(StatusCode, Json<IngestResponse>), ApiError> {
    payload
        .validate()
        .map_err(|error| ApiError::Validation(error.to_string()))?;

    if payload.readings.len() > MAX_READINGS_PER_BATCH {
        return Err(ApiError::Validation(format!(
            "readings: must contain at most {MAX_READINGS_PER_BATCH} items"
        )));
    }

    let now = Utc::now();
    let mut readings = Vec::with_capacity(payload.readings.len());
    for (idx, reading) in payload.readings.iter().enumerate() {
        if !reading.value.is_finite() {
            return Err(ApiError::Validation(format!(
                "readings[{idx}].value: must be a finite number"
            )));
        }
        if reading.key.trim().is_empty() || reading.key.len() > MAX_METRIC_KEY_LEN {
            return Err(ApiError::Validation(format!(
                "readings[{idx}].key: must be 1..={MAX_METRIC_KEY_LEN} chars"
            )));
        }
        if reading.ts > now + MAX_FUTURE_SKEW {
            return Err(ApiError::Validation(format!(
                "readings[{idx}].ts: cannot be more than 5 minutes in the future"
            )));
        }
        if !reading.metadata.is_object() {
            return Err(ApiError::Validation(format!(
                "readings[{idx}].metadata: must be a JSON object"
            )));
        }

        readings.push(fleetpulse_store::NewReading {
            ts: reading.ts,
            key: reading.key.clone(),
            value: reading.value,
            metadata: reading.metadata.clone(),
        });
    }

    let outcome = fleetpulse_store::ingest_batch(
        state.db.pool(),
        payload.batch_id,
        auth.device_id,
        &readings,
    )
    .await
    .map_err(|error| match error {
        fleetpulse_store::DbError::Conflict => ApiError::Conflict(
            "batch_id already used by a different device".to_owned(),
        ),
        other => {
            tracing::error!(%other, batch_id = %payload.batch_id, device_id = %auth.device_id, "telemetry ingest failed");
            ApiError::Unavailable
        }
    })?;

    let (status, body) = match outcome {
        fleetpulse_store::IngestOutcome::Created { accepted } => (
            StatusCode::CREATED,
            IngestResponse {
                status: "accepted",
                batch_id: payload.batch_id,
                accepted,
            },
        ),
        fleetpulse_store::IngestOutcome::Duplicate { accepted } => (
            StatusCode::OK,
            IngestResponse {
                status: "duplicate",
                batch_id: payload.batch_id,
                accepted,
            },
        ),
    };

    Ok((status, Json(body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_batch() {
        let payload = TelemetryBatchRequest {
            batch_id: Uuid::new_v4(),
            readings: vec![],
        };
        assert!(payload.validate().is_err());
    }

    #[test]
    fn default_metadata_is_empty_object() {
        let json = r#"{"ts":"2026-01-01T00:00:00Z","key":"temp","value":1.0}"#;
        let reading: ReadingRequest = serde_json::from_str(json).expect("deserializes");
        assert!(reading.metadata.is_object());
    }
}
