//! Shared extractors that keep every client error on the typed `ApiError`
//! envelope instead of leaking axum's default rejection bodies.

use axum::extract::{FromRequest, Request};
use axum::Json;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::error::ApiError;

/// JSON body that has also been checked with `validator`.
///
/// Handlers can then focus on the checks a derive cannot express (finite
/// numbers, timestamp skew, metadata shape) without repeating error plumbing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(payload) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| ApiError::Validation(rejection.to_string()))?;
        payload
            .validate()
            .map_err(|error| ApiError::Validation(error.to_string()))?;
        Ok(Self(payload))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::telemetry::TelemetryBatchRequest;

    #[tokio::test]
    async fn malformed_json_maps_to_validation_error() {
        let req = Request::post("/v1/telemetry")
            .header("content-type", "application/json")
            .body(Body::from("{not json"))
            .expect("build request");

        let error = ValidatedJson::<TelemetryBatchRequest>::from_request(req, &())
            .await
            .expect_err("malformed JSON is rejected");
        assert_eq!(error.code(), "validation_failed");
        assert_eq!(error.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn invalid_fields_map_to_validation_error() {
        let body = json!({
            "batch_id": Uuid::new_v4(),
            "readings": [{ "ts": "2026-01-01T00:00:00Z", "key": "", "value": 1.0 }]
        });
        let req = Request::post("/v1/telemetry")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("build request");

        let error = ValidatedJson::<TelemetryBatchRequest>::from_request(req, &())
            .await
            .expect_err("empty metric key is rejected");
        assert_eq!(error.code(), "validation_failed");
    }

    #[tokio::test]
    async fn valid_body_is_accepted() {
        let body = json!({
            "batch_id": Uuid::new_v4(),
            "readings": [{ "ts": "2026-01-01T00:00:00Z", "key": "temp", "value": 1.0 }]
        });
        let req = Request::post("/v1/telemetry")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("build request");

        let ValidatedJson(payload) = ValidatedJson::<TelemetryBatchRequest>::from_request(req, &())
            .await
            .expect("valid body");
        assert_eq!(payload.readings.len(), 1);
    }
}
