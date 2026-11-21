use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle state of a device.
///
/// Stored as text in Postgres with a `CHECK` constraint; converted through
/// [`FromStr`] in the store layer so the domain stays database-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Provisioning,
    Online,
    Offline,
    Retired,
}

impl DeviceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Retired => "retired",
        }
    }
}

impl fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown device status: {0}")]
pub struct UnknownDeviceStatus(pub String);

impl FromStr for DeviceStatus {
    type Err = UnknownDeviceStatus;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "provisioning" => Ok(Self::Provisioning),
            "online" => Ok(Self::Online),
            "offline" => Ok(Self::Offline),
            "retired" => Ok(Self::Retired),
            other => Err(UnknownDeviceStatus(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub status: DeviceStatus,
    pub firmware_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_text() {
        for status in [
            DeviceStatus::Provisioning,
            DeviceStatus::Online,
            DeviceStatus::Offline,
            DeviceStatus::Retired,
        ] {
            assert_eq!(status.as_str().parse::<DeviceStatus>(), Ok(status));
        }
    }

    #[test]
    fn unknown_status_is_rejected() {
        let error = "exploded".parse::<DeviceStatus>().unwrap_err();
        assert_eq!(error.0, "exploded");
    }
}
