mod common;

use axum::http::StatusCode;
use common::{get_json, get_status, setup, MOVIE_IMDB_ID, SERIES_IMDB_ID};
use lanio::auth::compute_token;

#[tokio::test]
async fn correct_token_grants_manifest_access() {
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (status, json) = get_json(&app, &format!("/{token}/manifest.json")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "lanio.local.com");
}

#[tokio::test]
async fn correct_token_grants_movie_catalog_access() {
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (status, json) =
        get_json(&app, &format!("/{token}/catalog/movie/lanio-movies")).await;
    assert_eq!(status, StatusCode::OK);
    let metas = json["metas"].as_array().unwrap();
    assert!(
        metas.iter().any(|m| m["id"] == MOVIE_IMDB_ID),
        "expected {MOVIE_IMDB_ID} in catalog with auth; got: {metas:?}"
    );
}

#[tokio::test]
async fn correct_token_grants_series_catalog_access() {
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (status, json) =
        get_json(&app, &format!("/{token}/catalog/series/lanio-series")).await;
    assert_eq!(status, StatusCode::OK);
    let metas = json["metas"].as_array().unwrap();
    assert!(
        metas.iter().any(|m| m["id"] == SERIES_IMDB_ID),
        "expected {SERIES_IMDB_ID} in catalog with auth; got: {metas:?}"
    );
}

#[tokio::test]
async fn correct_token_grants_stream_access() {
    let app = setup(Some("testpassword")).await;
    let token = compute_token("testpassword");
    let (status, json) =
        get_json(&app, &format!("/{token}/stream/movie/{MOVIE_IMDB_ID}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json["streams"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn wrong_token_returns_404() {
    let app = setup(Some("testpassword")).await;
    let status = get_status(&app, "/wrongtoken/manifest.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stremio_routes_unreachable_without_token_prefix() {
    // With auth enabled, routes move to /{token}/… so the bare path should 404.
    let app = setup(Some("testpassword")).await;
    let status = get_status(&app, "/manifest.json").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
