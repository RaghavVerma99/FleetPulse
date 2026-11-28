use chrono::{DateTime, Utc};
use fleetpulse_domain::{Device, DeviceStatus, Reading};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::DbError;

/// Database row for a device. The domain type owns the typed status; this
/// shadow struct exists only because `sqlx::FromRow` must be derived locally.
#[derive(Debug, FromRow)]
struct DeviceRow {
    id: Uuid,
    name: String,
    status: String,
    firmware_version: Option<String>,
    last_seen: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

fn to_device(row: DeviceRow) -> Result<Device, DbError> {
    let status = row
        .status
        .parse::<DeviceStatus>()
        .map_err(|error| DbError::InvalidData(error.to_string()))?;
    Ok(Device {
        id: row.id,
        name: row.name,
        status,
        firmware_version: row.firmware_version,
        last_seen: row.last_seen,
        created_at: row.created_at,
    })
}

/// Database row for a reading; converted into `domain::Reading` before it
/// leaves this crate.
#[derive(Debug, FromRow)]
struct ReadingRow {
    id: Uuid,
    device_id: Uuid,
    batch_id: Uuid,
    ts: DateTime<Utc>,
    key: String,
    value: f64,
    metadata: Value,
}

fn to_reading(row: ReadingRow) -> Reading {
    Reading {
        id: row.id,
        device_id: row.device_id,
        batch_id: row.batch_id,
        ts: row.ts,
        key: row.key,
        value: row.value,
        metadata: row.metadata,
    }
}

/// Query projection for "latest value per metric". It is not a full domain
/// entity (no `id` or `device_id`), so it stays store-local.
#[derive(Debug, Clone, FromRow)]
pub struct LatestReadingRow {
    pub key: String,
    pub value: f64,
    pub ts: DateTime<Utc>,
    pub batch_id: Uuid,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct DevicesQuery {
    pub limit: i64,
    pub before_ts: Option<DateTime<Utc>>,
    pub before_id: Option<Uuid>,
}

pub async fn list_devices(pool: &sqlx::PgPool, q: DevicesQuery) -> Result<Vec<Device>, DbError> {
    // Cursor is (created_at, id) for deterministic "newest first" pagination.
    let rows = sqlx::query_as::<_, DeviceRow>(
        r#"
        select id, name, status, firmware_version, last_seen, created_at
        from devices
        where ($1::timestamptz is null or (created_at, id) < ($1::timestamptz, $2::uuid))
        order by created_at desc, id desc
        limit $3
        "#,
    )
    .bind(q.before_ts)
    .bind(q.before_id)
    .bind(q.limit)
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(to_device).collect()
}

pub async fn get_device(pool: &sqlx::PgPool, device_id: Uuid) -> Result<Option<Device>, DbError> {
    let row = sqlx::query_as::<_, DeviceRow>(
        r#"
        select id, name, status, firmware_version, last_seen, created_at
        from devices
        where id = $1
        "#,
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Query)?;

    row.map(to_device).transpose()
}

#[derive(Debug, Clone, Copy)]
pub struct ReadingsQuery {
    pub device_id: Uuid,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: i64,
    pub before_ts: Option<DateTime<Utc>>,
    pub before_id: Option<Uuid>,
}

pub async fn list_readings(pool: &sqlx::PgPool, q: ReadingsQuery) -> Result<Vec<Reading>, DbError> {
    // Cursor is (ts, id) for deterministic pagination.
    let rows = sqlx::query_as::<_, ReadingRow>(
        r#"
        select id, device_id, batch_id, ts, key, value, metadata
        from readings
        where device_id = $1
          and ($2::timestamptz is null or ts >= $2)
          and ($3::timestamptz is null or ts <= $3)
          and (
            $4::timestamptz is null
            or (ts, id) < ($4::timestamptz, $5::uuid)
          )
        order by ts desc, id desc
        limit $6
        "#,
    )
    .bind(q.device_id)
    .bind(q.from)
    .bind(q.to)
    .bind(q.before_ts)
    .bind(q.before_id)
    .bind(q.limit)
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)?;

    Ok(rows.into_iter().map(to_reading).collect())
}

pub async fn latest_readings(
    pool: &sqlx::PgPool,
    device_id: Uuid,
) -> Result<Vec<LatestReadingRow>, DbError> {
    sqlx::query_as::<_, LatestReadingRow>(
        r#"
        select distinct on (key)
            key, value, ts, batch_id, metadata
        from readings
        where device_id = $1
        order by key asc, ts desc, id desc
        "#,
    )
    .bind(device_id)
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)
}
