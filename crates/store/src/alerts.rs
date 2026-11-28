//! Threshold rule evaluation and alert queries.
//!
//! Rules are evaluated inside the ingest transaction so an alert can never
//! outlive the readings that triggered it (ADR-015).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use fleetpulse_domain::{Alert, AlertRule, AlertSeverity, ThresholdOperator};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::db::DbError;

#[derive(Debug, FromRow)]
struct AlertRuleRow {
    id: Uuid,
    device_id: Uuid,
    metric_key: String,
    operator: String,
    threshold: f64,
    severity: String,
    cooldown_seconds: i32,
    enabled: bool,
    created_at: DateTime<Utc>,
}

fn to_rule(row: AlertRuleRow) -> Result<AlertRule, DbError> {
    let operator = row
        .operator
        .parse::<ThresholdOperator>()
        .map_err(|error| DbError::InvalidData(error.to_string()))?;
    let severity = row
        .severity
        .parse::<AlertSeverity>()
        .map_err(|error| DbError::InvalidData(error.to_string()))?;
    Ok(AlertRule {
        id: row.id,
        device_id: row.device_id,
        metric_key: row.metric_key,
        operator,
        threshold: row.threshold,
        severity,
        cooldown_seconds: row.cooldown_seconds,
        enabled: row.enabled,
        created_at: row.created_at,
    })
}

fn crosses(rule: &AlertRule, value: f64) -> bool {
    match rule.operator {
        ThresholdOperator::Gt => value > rule.threshold,
        ThresholdOperator::Gte => value >= rule.threshold,
        ThresholdOperator::Lt => value < rule.threshold,
        ThresholdOperator::Lte => value <= rule.threshold,
    }
}

/// Evaluate every enabled rule for this device against the newest value for
/// each metric in the batch. Returns the number of alerts fired.
pub async fn evaluate_rules(
    conn: &mut PgConnection,
    device_id: Uuid,
    latest_by_key: &HashMap<&str, (DateTime<Utc>, f64)>,
) -> Result<usize, DbError> {
    let rules = sqlx::query_as::<_, AlertRuleRow>(
        r#"
        select id, device_id, metric_key, operator, threshold, severity,
               cooldown_seconds, enabled, created_at
        from alert_rules
        where device_id = $1 and enabled
        "#,
    )
    .bind(device_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(DbError::Query)?
    .into_iter()
    .map(to_rule)
    .collect::<Result<Vec<_>, _>>()?;

    let mut fired = 0;
    for rule in rules {
        let Some(&(_, value)) = latest_by_key.get(rule.metric_key.as_str()) else {
            continue;
        };
        if !crosses(&rule, value) {
            continue;
        }
        if fired_recently(conn, rule.id, rule.cooldown_seconds).await? {
            continue;
        }

        let message = format!(
            "{} {} {} (value {})",
            rule.metric_key,
            rule.operator.symbol(),
            rule.threshold,
            value
        );
        sqlx::query(
            r#"
            insert into alerts (device_id, rule_id, severity, value, message)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(device_id)
        .bind(rule.id)
        .bind(rule.severity.as_str())
        .bind(value)
        .bind(message)
        .execute(&mut *conn)
        .await
        .map_err(DbError::Query)?;
        fired += 1;
    }

    Ok(fired)
}

async fn fired_recently(
    conn: &mut PgConnection,
    rule_id: Uuid,
    cooldown_seconds: i32,
) -> Result<bool, DbError> {
    sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1 from alerts
            where rule_id = $1
              and triggered_at > now() - ($2::int * interval '1 second')
        )
        "#,
    )
    .bind(rule_id)
    .bind(cooldown_seconds)
    .fetch_one(&mut *conn)
    .await
    .map_err(DbError::Query)
}

#[derive(Debug, FromRow)]
struct AlertRow {
    id: Uuid,
    device_id: Uuid,
    rule_id: Uuid,
    severity: String,
    value: f64,
    message: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

fn to_alert(row: AlertRow) -> Result<Alert, DbError> {
    let severity = row
        .severity
        .parse::<AlertSeverity>()
        .map_err(|error| DbError::InvalidData(error.to_string()))?;
    Ok(Alert {
        id: row.id,
        device_id: row.device_id,
        rule_id: row.rule_id,
        severity,
        value: row.value,
        message: row.message,
        triggered_at: row.triggered_at,
        resolved_at: row.resolved_at,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct AlertsQuery {
    pub device_id: Option<Uuid>,
    pub limit: i64,
    pub before_ts: Option<DateTime<Utc>>,
    pub before_id: Option<Uuid>,
}

pub async fn list_alerts(pool: &PgPool, q: AlertsQuery) -> Result<Vec<Alert>, DbError> {
    let rows = sqlx::query_as::<_, AlertRow>(
        r#"
        select id, device_id, rule_id, severity, value, message, triggered_at, resolved_at
        from alerts
        where ($1::uuid is null or device_id = $1)
          and (
            $2::timestamptz is null
            or (triggered_at, id) < ($2::timestamptz, $3::uuid)
          )
        order by triggered_at desc, id desc
        limit $4
        "#,
    )
    .bind(q.device_id)
    .bind(q.before_ts)
    .bind(q.before_id)
    .bind(q.limit)
    .fetch_all(pool)
    .await
    .map_err(DbError::Query)?;

    rows.into_iter().map(to_alert).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_comparisons() {
        let rule = AlertRule {
            id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            metric_key: "temp".to_owned(),
            operator: ThresholdOperator::Gte,
            threshold: 80.0,
            severity: AlertSeverity::Critical,
            cooldown_seconds: 300,
            enabled: true,
            created_at: Utc::now(),
        };
        assert!(crosses(&rule, 80.0));
        assert!(crosses(&rule, 85.0));
        assert!(!crosses(&rule, 79.9));

        let below = AlertRule {
            operator: ThresholdOperator::Lt,
            ..rule
        };
        assert!(crosses(&below, 10.0));
        assert!(!crosses(&below, 80.0));
    }
}
