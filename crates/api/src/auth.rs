use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Authenticated device identity extracted from `X-Device-Key`.
#[derive(Debug, Clone, Copy)]
pub struct DeviceAuth {
    pub device_id: Uuid,
}

impl FromRequestParts<AppState> for DeviceAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let presented_key = parts
            .headers
            .get("x-device-key")
            .ok_or(ApiError::Unauthorized)?
            .to_str()
            .map_err(|_| ApiError::Unauthorized)?;

        let device_id = fleetpulse_store::device_id_for_api_key(state.db.pool(), presented_key)
            .await
            .map_err(|error| {
                tracing::error!(%error, "device auth lookup failed");
                ApiError::Unavailable
            })?
            .ok_or(ApiError::Unauthorized)?;

        Ok(Self { device_id })
    }
}
