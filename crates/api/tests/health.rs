use axum::body::Body;
use axum::http::{Request, StatusCode};
use fleetpulse_api::app;
use fleetpulse_api::state::AppState;
use fleetpulse_store::Db;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn state(database_url: &str) -> AppState {
    let db = Db::connect_lazy(database_url, 1).expect("lazy pool accepts the URL");
    AppState {
        db,
        operator_api_key: "test-operator-key".to_owned(),
    }
}

#[tokio::test]
async fn liveness_succeeds_without_a_database() {
    let response = app(state(
        "postgres://fleetpulse:fleetpulse@127.0.0.1:1/fleetpulse",
    ))
    .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
    .await
    .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn readiness_fails_when_postgres_is_unreachable() {
    let response = app(state(
        "postgres://fleetpulse:fleetpulse@127.0.0.1:1/fleetpulse",
    ))
    .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
    .await
    .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(json["status"], "degraded");
    let postgres = json["checks"]["postgres"]
        .as_str()
        .expect("postgres check is a string");
    assert!(
        ["unavailable", "timeout"].contains(&postgres),
        "unexpected postgres status: {postgres}"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at a migrated database"]
async fn readiness_succeeds_when_postgres_is_up() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for this test");
    let db = Db::connect(&database_url, 1)
        .await
        .expect("connect to postgres");
    db.migrate().await.expect("apply migrations");

    let response = app(AppState {
        db,
        operator_api_key: "test-operator-key".to_owned(),
    })
    .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
    .await
    .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["checks"]["postgres"], "ok");
}
