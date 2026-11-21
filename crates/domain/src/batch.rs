use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Reading;

/// One upload from a device. `id` is client-supplied and doubles as the
/// idempotency key: replaying a batch must not duplicate its readings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryBatch {
    pub id: Uuid,
    pub device_id: Uuid,
    pub readings: Vec<Reading>,
}
