use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Every failure that can reach a client. Each variant owns both an HTTP
/// status and a stable machine-readable code, so responses stay predictable.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("service unavailable")]
    Unavailable,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unauthorized => "unauthorized",
            Self::Validation(_) => "validation_failed",
            Self::Unavailable => "service_unavailable",
            Self::Conflict(_) => "conflict",
            Self::Internal(_) => "internal_error",
        }
    }

    /// Internal error text can contain connection strings and SQL, so it is
    /// logged but never returned to the caller.
    fn public_message(&self) -> String {
        match self {
            Self::Internal(_) => "internal server error".to_owned(),
            other => other.to_string(),
        }
    }
}

/// Map a persistence error to a client-safe API error. Corrupt stored data is
/// an internal inconsistency (500); everything else is a dependency failure
/// (503) and is logged with its details.
pub fn from_db(error: fleetpulse_store::DbError) -> ApiError {
    tracing::error!(%error, "database query failed");
    match error {
        fleetpulse_store::DbError::InvalidData(_) => {
            ApiError::Internal(anyhow::anyhow!("invalid data returned from database"))
        }
        _ => ApiError::Unavailable,
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorDetail,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }

        let body = ErrorEnvelope {
            error: ErrorDetail {
                code: self.code(),
                message: self.public_message(),
            },
        };

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_variants_to_statuses() {
        assert_eq!(ApiError::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(ApiError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            ApiError::Validation("bad".to_owned()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ApiError::Unavailable.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            ApiError::Internal(anyhow::anyhow!("boom")).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let error = ApiError::Internal(anyhow::anyhow!("postgres://user:hunter2@db/internal"));
        assert_eq!(error.public_message(), "internal server error");
    }

    #[test]
    fn validation_errors_keep_their_message() {
        let error = ApiError::Validation("value must be finite".to_owned());
        assert_eq!(
            error.public_message(),
            "validation failed: value must be finite"
        );
    }
}
