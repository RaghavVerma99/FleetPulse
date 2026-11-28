use std::sync::OnceLock;

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use fleetpulse_api::app;
use fleetpulse_api::state::AppState;
use fleetpulse_store::Db;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use serial_test::serial;
use tower::ServiceExt;
use uuid::Uuid;

fn db() -> Option<Db> {
    static DB: OnceLock<Option<Db>> = OnceLock::new();
    DB.get_or_init(|| {
        let url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()?;
        Db::connect_lazy(&url, 10).ok()
    })
    .clone()
}

async fn migrated_db() -> Db {
    let db = db().expect("set TEST_DATABASE_URL or DATABASE_URL to run alert integration tests");
    db.migrate().await.expect("migrations apply");
    db
}

fn app_state(db: Db) -> AppState {
    AppState {
        db,
        operator_api_key: "test-operator-key".to_owned(),
    }
}

fn hash_key(raw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw.as_bytes(), &salt)
        .expect("hash key")
        .to_string()
}

async fn seed_device(db: &Db) -> (Uuid, String) {
    let device_id = Uuid::new_v4();
    let key_id = Uuid::new_v4();
    let raw_key = format!("fp_{}_{}", key_id.simple(), Uuid::new_v4().simple());
    sqlx::query("insert into devices (id, name, status) values ($1, $2, 'provisioning')")
        .bind(device_id)
        .bind(format!("alert-dev-{}", &raw_key[..8]))
        .execute(db.pool())
        .await
        .expect("insert device");
    sqlx::query("insert into api_keys (id, device_id, key_hash) values ($1, $2, $3)")
        .bind(key_id)
        .bind(device_id)
        .bind(hash_key(&raw_key))
        .execute(db.pool())
        .await
        .expect("insert api key");
    (device_id, raw_key)
}

async fn seed_rule(
    db: &Db,
    device_id: Uuid,
    metric_key: &str,
    operator: &str,
    threshold: f64,
    severity: &str,
    cooldown_seconds: i32,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        r#"
        insert into alert_rules
            (id, device_id, metric_key, operator, threshold, severity, cooldown_seconds)
        values ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(rule_id)
    .bind(device_id)
    .bind(metric_key)
    .bind(operator)
    .bind(threshold)
    .bind(severity)
    .bind(cooldown_seconds)
    .execute(db.pool())
    .await
    .expect("insert alert rule");
    rule_id
}

async fn post_batch(db: &Db, key: &str, batch_id: Uuid, key_name: &str, value: f64) -> StatusCode {
    let body = json!({
        "batch_id": batch_id,
        "readings": [{ "ts": Utc::now().to_rfc3339(), "key": key_name, "value": value }]
    });
    let response = app(app_state(db.clone()))
        .oneshot(
            Request::post("/v1/telemetry")
                .header("content-type", "application/json")
                .header("x-device-key", key)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");
    response.status()
}

async fn get_alerts(db: &Db, query: &str) -> (StatusCode, Value) {
    let response = app(app_state(db.clone()))
        .oneshot(
            Request::get(format!("/v1/alerts?{query}"))
                .header("x-operator-key", "test-operator-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes) }));
    (status, json)
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn rule_fires_and_cooldown_suppresses_repeat() {
    let db = migrated_db().await;
    let (device_id, key) = seed_device(&db).await;
    seed_rule(&db, device_id, "temp", "gte", 80.0, "critical", 300).await;

    assert_eq!(
        post_batch(&db, &key, Uuid::new_v4(), "temp", 85.0).await,
        StatusCode::CREATED
    );
    let (status, json) = get_alerts(&db, &format!("device_id={device_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["severity"], "critical");
    assert_eq!(items[0]["value"], 85.0);
    assert!(items[0]["message"].as_str().unwrap().contains("temp"));

    assert_eq!(
        post_batch(&db, &key, Uuid::new_v4(), "temp", 90.0).await,
        StatusCode::CREATED
    );
    let (_, json) = get_alerts(&db, &format!("device_id={device_id}")).await;
    assert_eq!(json["items"].as_array().unwrap().len(), 1);

    // A value below the threshold must not fire either.
    assert_eq!(
        post_batch(&db, &key, Uuid::new_v4(), "temp", 79.0).await,
        StatusCode::CREATED
    );
    let (_, json) = get_alerts(&db, &format!("device_id={device_id}")).await;
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn rules_are_scoped_by_device_and_metric() {
    let db = migrated_db().await;
    let (first_id, first_key) = seed_device(&db).await;
    let (second_id, second_key) = seed_device(&db).await;
    seed_rule(&db, first_id, "temp", "gte", 80.0, "warning", 0).await;
    seed_rule(&db, second_id, "temp", "gte", 80.0, "warning", 0).await;

    assert_eq!(
        post_batch(&db, &first_key, Uuid::new_v4(), "speed", 99.0).await,
        StatusCode::CREATED
    );
    let (_, json) = get_alerts(&db, &format!("device_id={first_id}")).await;
    assert!(json["items"].as_array().unwrap().is_empty());

    assert_eq!(
        post_batch(&db, &first_key, Uuid::new_v4(), "temp", 88.0).await,
        StatusCode::CREATED
    );
    let (_, json) = get_alerts(&db, &format!("device_id={first_id}")).await;
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    let (_, json) = get_alerts(&db, &format!("device_id={second_id}")).await;
    assert!(json["items"].as_array().unwrap().is_empty());

    // The second device's own rule fires independently.
    assert_eq!(
        post_batch(&db, &second_key, Uuid::new_v4(), "temp", 81.0).await,
        StatusCode::CREATED
    );
    let (_, json) = get_alerts(&db, "").await;
    assert_eq!(json["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn alerts_are_cursor_paginated() {
    let db = migrated_db().await;
    let (device_id, key) = seed_device(&db).await;
    seed_rule(&db, device_id, "temp", "gte", 80.0, "info", 0).await;

    for value in [85.0, 86.0] {
        assert_eq!(
            post_batch(&db, &key, Uuid::new_v4(), "temp", value).await,
            StatusCode::CREATED
        );
    }

    let (status, json) = get_alerts(&db, "limit=1").await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let first_id = items[0]["id"].as_str().unwrap().to_owned();
    let next = json["next_before"].as_str().expect("cursor").to_owned();

    let (status, json) = get_alerts(&db, &format!("limit=1&before={}", urlencoding(&next))).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_ne!(items[0]["id"].as_str().unwrap(), first_id);
    assert!(json["next_before"].is_null());
}
