use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::DbError;

#[derive(Debug, Clone, FromRow)]
pub struct DeviceRow {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub firmware_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReadingRow {
    pub id: Uuid,
    pub device_id: Uuid,
    pub batch_id: Uuid,
    pub ts: DateTime<Utc>,
    pub key: String,
    pub value: f64,
    pub metadata: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct LatestReadingRow {
    pub key: String,
    pub value: f64,
    pub ts: DateTime<Utc>,
    pub batch_id: Uuid,
    pub metadata: Value,
}

pub async fn list_devices(pool: &sqlx::PgPool) -> Result<Vec<DeviceRow>, DbError> {
    sqlx::query_as::<_, DeviceRow>(
        r#"
        select id, name, status, firmware_version, last_seen, created_at
        from devices
        order by created_at desc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)
}

pub async fn get_device(
    pool: &sqlx::PgPool,
    device_id: Uuid,
) -> Result<Option<DeviceRow>, DbError> {
    sqlx::query_as::<_, DeviceRow>(
        r#"
        select id, name, status, firmware_version, last_seen, created_at
        from devices
        where id = $1
        "#,
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(DbError::Query)
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

pub async fn list_readings(
    pool: &sqlx::PgPool,
    q: ReadingsQuery,
) -> Result<Vec<ReadingRow>, DbError> {
    // Cursor is (ts, id) for deterministic pagination.
    sqlx::query_as::<_, ReadingRow>(
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
    .map_err(DbError::Query)
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
