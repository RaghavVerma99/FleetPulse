use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::DbError;

/// Look up the device owning a raw API key.
///
/// The `api_keys.key_hash` column stores a PHC string (Argon2id). We verify in
/// Rust rather than inside SQL so the hash algorithm can be upgraded without a
/// migration. The active-key index keeps the candidate set small.
pub async fn device_id_for_api_key(
    pool: &PgPool,
    presented_key: &str,
) -> Result<Option<Uuid>, DbError> {
    let candidates: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        select device_id, key_hash
        from api_keys
        where revoked_at is null
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)?;

    for (device_id, key_hash) in candidates {
        let parsed = match PasswordHash::new(&key_hash) {
            Ok(hash) => hash,
            Err(error) => {
                tracing::warn!(%error, %device_id, "skipping malformed api key hash");
                continue;
            }
        };

        if Argon2::default()
            .verify_password(presented_key.as_bytes(), &parsed)
            .is_ok()
        {
            return Ok(Some(device_id));
        }
    }

    Ok(None)
}
