mod common;

use axum::http::StatusCode;
use common::{get_json, setup};

#[tokio::test]
async fn manifest_accessible_without_auth() {
    let app = setup(None).await;
    let (status, json) = get_json(&app, "/manifest.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "lanio.local.com");
    assert!(json["catalogs"].as_array().unwrap().len() >= 2);
}
