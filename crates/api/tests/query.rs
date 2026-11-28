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
    let db = db().expect("set TEST_DATABASE_URL or DATABASE_URL to run integration tests");
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
    let raw_key = format!("fp_test_{}", Uuid::new_v4().simple());
    sqlx::query("insert into devices (id, name, status) values ($1, $2, 'provisioning')")
        .bind(device_id)
        .bind(format!("dev-{}", &raw_key[..8]))
        .execute(db.pool())
        .await
        .expect("insert device");
    sqlx::query("insert into api_keys (id, device_id, key_hash) values ($1, $2, $3)")
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(hash_key(&raw_key))
        .execute(db.pool())
        .await
        .expect("insert key");
    (device_id, raw_key)
}

async fn send(app_router: axum::Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let res = app_router.oneshot(req).await.expect("response");
    let status = res.status();
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes) }));
    (status, json)
}

async fn post_telemetry(db: &Db, device_key: &str, batch_id: Uuid, readings: serde_json::Value) {
    let body = json!({ "batch_id": batch_id, "readings": readings });
    let req = Request::post("/v1/telemetry")
        .header("content-type", "application/json")
        .header("x-device-key", device_key)
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn operator_can_list_and_get_device() {
    let db = migrated_db().await;
    let (device_id, _key) = seed_device(&db).await;

    let req = Request::get("/v1/devices")
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == device_id.to_string()));

    let req = Request::get(format!("/v1/devices/{device_id}"))
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], device_id.to_string());

    let req = Request::get(format!("/v1/devices/{device_id}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app(app_state(db)), req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn readings_query_supports_range_cursor_and_latest() {
    let db = migrated_db().await;
    let (device_id, key) = seed_device(&db).await;
    let base = Utc::now() - Duration::minutes(10);

    for i in 0..3 {
        let ts = (base + Duration::minutes(i)).to_rfc3339();
        post_telemetry(
            &db,
            &key,
            Uuid::new_v4(),
            json!([
                {"ts": ts, "key": "temp", "value": 20.0 + i as f64, "metadata": {"i": i}},
                {"ts": ts, "key": "speed", "value": 10.0 * i as f64}
            ]),
        )
        .await;
    }

    let req = Request::get(format!("/v1/devices/{device_id}/readings?limit=2"))
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let next = json["next_before"].as_str().unwrap().to_string();

    let req = Request::get(format!(
        "/v1/devices/{device_id}/readings?limit=2&before={}",
        urlencoding(next.as_str())
    ))
    .header("x-operator-key", "test-operator-key")
    .body(Body::empty())
    .unwrap();
    let (status, json) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["items"].as_array().unwrap().len() <= 2);

    let req = Request::get(format!("/v1/devices/{device_id}/readings/latest"))
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db)), req).await;
    assert_eq!(status, StatusCode::OK);
    let arr = json.as_array().unwrap();
    assert!(arr.iter().any(|r| r["key"] == "temp"));
    assert!(arr.iter().any(|r| r["key"] == "speed"));
}

#[tokio::test]
#[serial]
#[ignore = "requires TEST_DATABASE_URL or DATABASE_URL"]
async fn devices_list_is_cursor_paginated() {
    let db = migrated_db().await;
    let (_first, _) = seed_device(&db).await;
    let (_second, _) = seed_device(&db).await;

    let req = Request::get("/v1/devices?limit=1")
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db.clone())), req).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let first_id = items[0]["id"].as_str().unwrap().to_owned();
    let next = json["next_before"].as_str().expect("cursor").to_owned();

    let req = Request::get(format!("/v1/devices?limit=1&before={}", urlencoding(&next)))
        .header("x-operator-key", "test-operator-key")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(app_state(db)), req).await;
    assert_eq!(status, StatusCode::OK);
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_ne!(items[0]["id"].as_str().unwrap(), first_id);
}

fn urlencoding(s: &str) -> String {
    // minimal percent-encoding for test cursor
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
