use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::DbError;

#[derive(Debug, Clone)]
pub struct NewReading {
    pub ts: DateTime<Utc>,
    pub key: String,
    pub value: f64,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    Created { accepted: usize },
    Duplicate { accepted: usize },
}

/// Insert a telemetry batch atomically.
///
/// The batch row is the idempotency gate: `ON CONFLICT (id) DO NOTHING`.
/// If the insert claims zero rows, the batch was already accepted and we return
/// `Duplicate` with the stored reading count. Otherwise readings and the device
/// `last_seen` update commit in the same transaction.
pub async fn ingest_batch(
    pool: &PgPool,
    batch_id: Uuid,
    device_id: Uuid,
    readings: &[NewReading],
) -> Result<IngestOutcome, DbError> {
    let mut tx = pool.begin().await.map_err(DbError::Query)?;

    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        insert into telemetry_batches (id, device_id, reading_count)
        values ($1, $2, $3)
        on conflict (id) do nothing
        returning id
        "#,
    )
    .bind(batch_id)
    .bind(device_id)
    .bind(readings.len() as i32)
    .fetch_optional(&mut *tx)
    .await
    .map_err(DbError::Query)?;

    if inserted.is_none() {
        let existing_count = sqlx::query_scalar::<_, i32>(
            r#"
            select reading_count
            from telemetry_batches
            where id = $1 and device_id = $2
            "#,
        )
        .bind(batch_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::Query)?;

        return match existing_count {
            Some(count) => Ok(IngestOutcome::Duplicate {
                accepted: count as usize,
            }),
            // Same batch_id claimed by a different device: treat as duplicate-shaped
            // conflict; the handler maps this to a validation error.
            None => Err(DbError::Conflict),
        };
    }

    let mut device_ids = Vec::with_capacity(readings.len());
    let mut batch_ids = Vec::with_capacity(readings.len());
    let mut tss = Vec::with_capacity(readings.len());
    let mut keys = Vec::with_capacity(readings.len());
    let mut values = Vec::with_capacity(readings.len());
    let mut metadata = Vec::with_capacity(readings.len());
    for reading in readings {
        device_ids.push(device_id);
        batch_ids.push(batch_id);
        tss.push(reading.ts);
        keys.push(reading.key.clone());
        values.push(reading.value);
        metadata.push(reading.metadata.clone());
    }

    sqlx::query(
        r#"
        insert into readings (device_id, batch_id, ts, key, value, metadata)
        select * from unnest($1::uuid[], $2::uuid[], $3::timestamptz[], $4::text[], $5::float8[], $6::jsonb[])
        "#,
    )
    .bind(&device_ids)
    .bind(&batch_ids)
    .bind(&tss)
    .bind(&keys)
    .bind(&values)
    .bind(&metadata)
    .execute(&mut *tx)
    .await
    .map_err(DbError::Query)?;

    sqlx::query(
        r#"
        update devices
        set last_seen = now(),
            status = case when status = 'retired' then status else 'online' end
        where id = $1
        "#,
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await
    .map_err(DbError::Query)?;

    // Evaluate rules against the newest value for each metric in this batch,
    // in the same transaction (ADR-015).
    let mut latest_by_key: HashMap<&str, (DateTime<Utc>, f64)> = HashMap::new();
    for reading in readings {
        latest_by_key
            .entry(reading.key.as_str())
            .and_modify(|current| {
                if reading.ts >= current.0 {
                    *current = (reading.ts, reading.value);
                }
            })
            .or_insert((reading.ts, reading.value));
    }
    crate::alerts::evaluate_rules(&mut *tx, device_id, &latest_by_key).await?;

    tx.commit().await.map_err(DbError::Query)?;

    Ok(IngestOutcome::Created {
        accepted: readings.len(),
    })
}
