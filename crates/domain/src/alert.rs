use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a rule compares a reading's value with its threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdOperator {
    Gt,
    Gte,
    Lt,
    Lte,
}

impl ThresholdOperator {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
        }
    }
}

impl fmt::Display for ThresholdOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown threshold operator: {0}")]
pub struct UnknownThresholdOperator(pub String);

impl FromStr for ThresholdOperator {
    type Err = UnknownThresholdOperator;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "gt" => Ok(Self::Gt),
            "gte" => Ok(Self::Gte),
            "lt" => Ok(Self::Lt),
            "lte" => Ok(Self::Lte),
            other => Err(UnknownThresholdOperator(other.to_owned())),
        }
    }
}

/// Severity assigned to an alert fired by a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl AlertSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown alert severity: {0}")]
pub struct UnknownAlertSeverity(pub String);

impl FromStr for AlertSeverity {
    type Err = UnknownAlertSeverity;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "critical" => Ok(Self::Critical),
            other => Err(UnknownAlertSeverity(other.to_owned())),
        }
    }
}

/// A threshold rule owned by one device. Cooldown is in seconds; repeat fires
/// within that window are suppressed (ADR-015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub device_id: Uuid,
    pub metric_key: String,
    pub operator: ThresholdOperator,
    pub threshold: f64,
    pub severity: AlertSeverity,
    pub cooldown_seconds: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// An alert fired by a rule during ingest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub device_id: Uuid,
    pub rule_id: Uuid,
    pub severity: AlertSeverity,
    pub value: f64,
    pub message: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_round_trips_through_text() {
        for op in [
            ThresholdOperator::Gt,
            ThresholdOperator::Gte,
            ThresholdOperator::Lt,
            ThresholdOperator::Lte,
        ] {
            assert_eq!(op.as_str().parse::<ThresholdOperator>(), Ok(op));
        }
    }

    #[test]
    fn severity_round_trips_through_text() {
        for severity in [
            AlertSeverity::Info,
            AlertSeverity::Warning,
            AlertSeverity::Critical,
        ] {
            assert_eq!(severity.as_str().parse::<AlertSeverity>(), Ok(severity));
        }
    }
}
