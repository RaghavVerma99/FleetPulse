use std::sync::OnceLock;

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use fleetpulse_api::app;
use fleetpulse_api::state::AppState;
use fleetpulse_store::Db;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

fn db() -> Option<Db> {
    static DB: OnceLock<Option<Db>> = OnceLock::new();
    DB.get_or_init(|| {
        let url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()?;
        let db = Db::connect_lazy(&url, 5).ok()?;
        Some(db)
    })
    .clone()
}

async fn migrated_db() -> Db {
    let db =
        db().expect("set TEST_DATABASE_URL or DATABASE_URL to run telemetry integration tests");
    db.migrate().await.expect("migrations apply");
    db
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
    let raw_key = format!("fp_test_{}", Uuid::new_v4().simple());
    let key_hash = hash_key(&raw_key);

    sqlx::query("insert into devices (id, name, status) values ($1, $2, 'provisioning')")
        .bind(device_id)
        .bind(format!("device-{}", &raw_key[..12]))
        .execute(db.pool())
        .await
        .expect("insert device");

    sqlx::query("insert into api_keys (id, device_id, key_hash) values ($1, $2, $3)")
        .bind(key_id)
        .bind(device_id)
        .bind(key_hash)
        .execute(db.pool())
        .await
        .expect("insert api key");

    (device_id, raw_key)
}

async fn post_batch(
    app_router: axum::Router,
    api_key: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app_router
        .oneshot(
            Request::post("/v1/telemetry")
                .header("content-type", "application/json")
                .header("x-device-key", api_key)
                .body(Body::from(body.to_string()))
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
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    (status, json)
}

fn sample_batch(batch_id: Uuid) -> serde_json::Value {
    json!({
        "batch_id": batch_id,
        "readings": [
            { "ts": Utc::now().to_rfc3339(), "key": "battery_temp", "value": 37.5, "metadata": {"unit":"C"} },
            { "ts": Utc::now().to_rfc3339(), "key": "speed_kph", "value": 52.0 }
        ]
    })
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn ingest_creates_then_dedupes_by_batch_id() {
    let db = migrated_db().await;
    let (device_id, api_key) = seed_device(&db).await;
    let batch_id = Uuid::new_v4();
    let body = sample_batch(batch_id);

    let (status, json) = post_batch(app(AppState { db: db.clone() }), &api_key, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["status"], "accepted");
    assert_eq!(json["accepted"], 2);

    let (status, json) = post_batch(app(AppState { db: db.clone() }), &api_key, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "duplicate");
    assert_eq!(json["accepted"], 2);

    let count: i64 = sqlx::query_scalar("select count(*) from readings where device_id = $1")
        .bind(device_id)
        .fetch_one(db.pool())
        .await
        .expect("count");
    assert_eq!(count, 2);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn ingest_rejects_missing_or_bad_key() {
    let db = migrated_db().await;
    let body = sample_batch(Uuid::new_v4());

    let response = app(AppState { db: db.clone() })
        .oneshot(
            Request::post("/v1/telemetry")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let (status, json) = post_batch(app(AppState { db }), "fp_bad_key", body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"]["code"], "unauthorized");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn ingest_rejects_invalid_payloads() {
    let db = migrated_db().await;
    let (_device_id, api_key) = seed_device(&db).await;

    let future = Utc::now() + Duration::minutes(10);
    let bad = json!({
        "batch_id": Uuid::new_v4(),
        "readings": [{ "ts": future.to_rfc3339(), "key": "temp", "value": 1.0 }]
    });
    let (status, json) = post_batch(app(AppState { db: db.clone() }), &api_key, bad).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "validation_failed");

    let nan = json!({
        "batch_id": Uuid::new_v4(),
        "readings": [{ "ts": Utc::now().to_rfc3339(), "key": "temp", "value": null }]
    });
    // null is a deserialization/type error - axum returns 400/422 depending; accept either as client error
    let (status, _) = post_batch(app(AppState { db: db.clone() }), &api_key, nan).await;
    assert!(status.is_client_error());

    let empty = json!({ "batch_id": Uuid::new_v4(), "readings": [] });
    let (status, _) = post_batch(app(AppState { db }), &api_key, empty).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn ingest_rejects_cross_device_batch_id_reuse() {
    let db = migrated_db().await;
    let (_d1, key1) = seed_device(&db).await;
    let (_d2, key2) = seed_device(&db).await;
    let batch_id = Uuid::new_v4();
    let body = sample_batch(batch_id);

    let (status, _) = post_batch(app(AppState { db: db.clone() }), &key1, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, json) = post_batch(app(AppState { db }), &key2, body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"]["code"], "conflict");
}
