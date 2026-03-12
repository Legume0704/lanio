mod common;

use axum::http::StatusCode;
use common::{get_json, setup};

#[tokio::test]
async fn health_returns_ok() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn health_always_accessible_when_auth_enabled() {
    let app = setup(Some("testpassword")).await;
    let (status, json) = get_json(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}
