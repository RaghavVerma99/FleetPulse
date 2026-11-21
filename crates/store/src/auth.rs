use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::DbError;

/// Look up the device owning a raw API key.
///
/// `api_keys.key_hash` stores a PHC string (Argon2id). Keys include an embedded
/// `key_id` prefix (`fp_<uuid>_<secret>`) so we can select one active row by
/// primary key instead of scanning/verifying all keys. Legacy keys without a
/// parseable prefix fall back to the old scan for compatibility.
pub async fn device_id_for_api_key(
    pool: &PgPool,
    presented_key: &str,
) -> Result<Option<Uuid>, DbError> {
    if let Some(key_id) = parse_key_id(presented_key) {
        let row: Option<(Uuid, String)> = sqlx::query_as(
            r#"
            select device_id, key_hash
            from api_keys
            where id = $1 and revoked_at is null
            "#,
        )
        .bind(key_id)
        .fetch_optional(pool)
        .await
        .map_err(DbError::Query)?;

        let Some((device_id, key_hash)) = row else {
            return Ok(None);
        };
        return Ok(verify_hash(device_id, &key_hash, presented_key).then_some(device_id));
    }

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
        if verify_hash(device_id, &key_hash, presented_key) {
            return Ok(Some(device_id));
        }
    }

    Ok(None)
}

pub fn generate_device_key(key_id: Uuid) -> String {
    let secret = Uuid::new_v4().simple().to_string();
    format!("fp_{}_{}", key_id.simple(), secret)
}

pub fn hash_device_key(raw_key: &str) -> Result<String, DbError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw_key.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| DbError::Hash(e.to_string()))
}

fn parse_key_id(raw_key: &str) -> Option<Uuid> {
    let mut parts = raw_key.split('_');
    if parts.next()? != "fp" {
        return None;
    }
    let id_part = parts.next()?;
    Uuid::parse_str(id_part).ok()
}

fn verify_hash(device_id: Uuid, key_hash: &str, presented_key: &str) -> bool {
    let parsed = match PasswordHash::new(key_hash) {
        Ok(hash) => hash,
        Err(error) => {
            tracing::warn!(%error, %device_id, "skipping malformed api key hash");
            return false;
        }
    };

    Argon2::default()
        .verify_password(presented_key.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_parseable_prefixed_key() {
        let key_id = Uuid::new_v4();
        let raw = generate_device_key(key_id);
        assert_eq!(parse_key_id(&raw), Some(key_id));
    }

    #[test]
    fn hash_round_trip_verifies() {
        let key_id = Uuid::new_v4();
        let raw = generate_device_key(key_id);
        let hash = hash_device_key(&raw).expect("hash");
        assert!(verify_hash(Uuid::new_v4(), &hash, &raw));
        assert!(!verify_hash(Uuid::new_v4(), &hash, "fp_bad"));
    }
}
