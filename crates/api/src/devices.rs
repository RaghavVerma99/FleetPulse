use axum::extract::{Path, Query, State};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use fleetpulse_domain::{Device, DeviceStatus, Reading};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::OperatorAuth;
use crate::error::ApiError;
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/devices", axum::routing::get(list_devices))
        .route("/v1/devices/{device_id}", axum::routing::get(get_device))
        .route(
            "/v1/devices/{device_id}/readings",
            axum::routing::get(list_readings),
        )
        .route(
            "/v1/devices/{device_id}/readings/latest",
            axum::routing::get(latest_readings),
        )
}

#[derive(Debug, Deserialize)]
pub struct DevicesParams {
    pub limit: Option<i64>,
    /// Cursor as `created_at_rfc3339|uuid`.
    pub before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReadingsParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    /// Cursor as `ts_rfc3339|uuid`.
    pub before: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    pub id: Uuid,
    pub name: String,
    pub status: DeviceStatus,
    pub firmware_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<Device> for DeviceResponse {
    fn from(device: Device) -> Self {
        Self {
            id: device.id,
            name: device.name,
            status: device.status,
            firmware_version: device.firmware_version,
            last_seen: device.last_seen,
            created_at: device.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DevicesResponse {
    pub items: Vec<DeviceResponse>,
    pub next_before: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadingResponse {
    pub id: Uuid,
    pub device_id: Uuid,
    pub batch_id: Uuid,
    pub ts: DateTime<Utc>,
    pub key: String,
    pub value: f64,
    pub metadata: Value,
}

impl From<Reading> for ReadingResponse {
    fn from(reading: Reading) -> Self {
        Self {
            id: reading.id,
            device_id: reading.device_id,
            batch_id: reading.batch_id,
            ts: reading.ts,
            key: reading.key,
            value: reading.value,
            metadata: reading.metadata,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LatestReadingResponse {
    pub key: String,
    pub value: f64,
    pub ts: DateTime<Utc>,
    pub batch_id: Uuid,
    pub metadata: Value,
}

impl From<fleetpulse_store::LatestReadingRow> for LatestReadingResponse {
    fn from(row: fleetpulse_store::LatestReadingRow) -> Self {
        Self {
            key: row.key,
            value: row.value,
            ts: row.ts,
            batch_id: row.batch_id,
            metadata: row.metadata,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReadingsResponse {
    pub items: Vec<ReadingResponse>,
    pub next_before: Option<String>,
}

async fn list_devices(
    State(state): State<AppState>,
    _auth: OperatorAuth,
    Query(params): Query<DevicesParams>,
) -> Result<Json<DevicesResponse>, ApiError> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::Validation(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }

    let (before_ts, before_id) = parse_before(params.before.as_deref())?;
    let mut devices = fleetpulse_store::list_devices(
        state.db.pool(),
        fleetpulse_store::DevicesQuery {
            limit: limit + 1,
            before_ts,
            before_id,
        },
    )
    .await
    .map_err(db_err)?;

    let next_before = if devices.len() as i64 > limit {
        devices.truncate(limit as usize);
        devices
            .last()
            .map(|d| format!("{}|{}", d.created_at.to_rfc3339(), d.id))
    } else {
        None
    };

    Ok(Json(DevicesResponse {
        items: devices.into_iter().map(Into::into).collect(),
        next_before,
    }))
}

async fn get_device(
    State(state): State<AppState>,
    _auth: OperatorAuth,
    Path(device_id): Path<Uuid>,
) -> Result<Json<DeviceResponse>, ApiError> {
    let device = fleetpulse_store::get_device(state.db.pool(), device_id)
        .await
        .map_err(db_err)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(device.into()))
}

async fn list_readings(
    State(state): State<AppState>,
    _auth: OperatorAuth,
    Path(device_id): Path<Uuid>,
    Query(params): Query<ReadingsParams>,
) -> Result<Json<ReadingsResponse>, ApiError> {
    ensure_device_exists(&state, device_id).await?;

    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::Validation(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    if let (Some(from), Some(to)) = (params.from, params.to) {
        if from > to {
            return Err(ApiError::Validation("from must be <= to".to_owned()));
        }
    }

    let (before_ts, before_id) = parse_before(params.before.as_deref())?;

    let mut items = fleetpulse_store::list_readings(
        state.db.pool(),
        fleetpulse_store::ReadingsQuery {
            device_id,
            from: params.from,
            to: params.to,
            limit: limit + 1,
            before_ts,
            before_id,
        },
    )
    .await
    .map_err(db_err)?;

    let next_before = if items.len() as i64 > limit {
        items.truncate(limit as usize);
        items
            .last()
            .map(|r| format!("{}|{}", r.ts.to_rfc3339(), r.id))
    } else {
        None
    };

    Ok(Json(ReadingsResponse {
        items: items.into_iter().map(Into::into).collect(),
        next_before,
    }))
}

async fn latest_readings(
    State(state): State<AppState>,
    _auth: OperatorAuth,
    Path(device_id): Path<Uuid>,
) -> Result<Json<Vec<LatestReadingResponse>>, ApiError> {
    ensure_device_exists(&state, device_id).await?;
    let items = fleetpulse_store::latest_readings(state.db.pool(), device_id)
        .await
        .map_err(db_err)?;
    Ok(Json(items.into_iter().map(Into::into).collect()))
}

async fn ensure_device_exists(state: &AppState, device_id: Uuid) -> Result<(), ApiError> {
    let exists = fleetpulse_store::get_device(state.db.pool(), device_id)
        .await
        .map_err(db_err)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

fn parse_before(before: Option<&str>) -> Result<(Option<DateTime<Utc>>, Option<Uuid>), ApiError> {
    let Some(before) = before else {
        return Ok((None, None));
    };
    let (ts_raw, id_raw) = before.split_once('|').ok_or_else(|| {
        ApiError::Validation("before must be formatted as <rfc3339>|<uuid>".to_owned())
    })?;
    let ts = DateTime::parse_from_rfc3339(ts_raw)
        .map_err(|_| ApiError::Validation("before timestamp must be RFC3339".to_owned()))?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id_raw)
        .map_err(|_| ApiError::Validation("before id must be a UUID".to_owned()))?;
    Ok((Some(ts), Some(id)))
}

fn db_err(error: fleetpulse_store::DbError) -> ApiError {
    tracing::error!(%error, "database query failed");
    match error {
        // A value the DB constraint should have prevented is an internal
        // inconsistency, not a transient dependency failure.
        fleetpulse_store::DbError::InvalidData(_) => {
            ApiError::Internal(anyhow::anyhow!("invalid data returned from database"))
        }
        _ => ApiError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cursor() {
        let id = Uuid::new_v4();
        let ts = Utc::now();
        let raw = format!("{}|{}", ts.to_rfc3339(), id);
        let (parsed_ts, parsed_id) = parse_before(Some(&raw)).expect("parse");
        assert_eq!(parsed_id, Some(id));
        assert_eq!(parsed_ts.unwrap().timestamp(), ts.timestamp());
    }

    #[test]
    fn rejects_bad_cursor() {
        let err = parse_before(Some("nope")).unwrap_err();
        assert_eq!(err.code(), "validation_failed");
    }

    #[test]
    fn device_status_serializes_as_snake_case() {
        let device = Device {
            id: Uuid::new_v4(),
            name: "demo".to_owned(),
            status: DeviceStatus::Online,
            firmware_version: None,
            last_seen: None,
            created_at: Utc::now(),
        };
        let json = serde_json::to_value(DeviceResponse::from(device)).expect("serialize");
        assert_eq!(json["status"], "online");
    }
}
