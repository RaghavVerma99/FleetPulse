//! Shared query-parameter rules for the operator list endpoints.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::ApiError;

pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;

pub fn validate_limit(limit: i64) -> Result<i64, ApiError> {
    if (1..=MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(ApiError::Validation(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )))
    }
}

/// Parse a `<rfc3339>|<uuid>` cursor used by every list endpoint.
pub fn parse_before(
    before: Option<&str>,
) -> Result<(Option<DateTime<Utc>>, Option<Uuid>), ApiError> {
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
        let error = parse_before(Some("nope")).unwrap_err();
        assert_eq!(error.code(), "validation_failed");
    }

    #[test]
    fn rejects_invalid_limits() {
        assert!(validate_limit(0).is_err());
        assert!(validate_limit(MAX_LIMIT + 1).is_err());
        assert_eq!(validate_limit(10), Ok(10));
    }
}
